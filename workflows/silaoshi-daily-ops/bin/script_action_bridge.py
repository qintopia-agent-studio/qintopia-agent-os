#!/usr/bin/env python3
"""Durable bridge from Hermes webhook scripts to one allowlisted Silaoshi action."""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import os
import re
import secrets
import socket
import sqlite3
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


MAX_PAYLOAD_BYTES = 64 * 1024
MAX_RESPONSE_BYTES = 8 * 1024
DEFAULT_CONFIG = "/etc/qintopia/silaoshi-script-action.json"
DELIVERY_KEYS = ("delivery_id", "event_id", "record_id", "recordId", "id")
RECORD_ID_RE = re.compile(r"\brec[a-zA-Z0-9]{8,}\b")


class BridgeError(RuntimeError):
    pass


def _read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise BridgeError("configuration must be a JSON object")
    return value


def load_config(path: Path) -> dict[str, Any]:
    cfg = _read_json(path)
    required = ("socket_path", "database_path", "secret_file", "action")
    if any(not cfg.get(key) for key in required):
        raise BridgeError("configuration is incomplete")
    action = cfg["action"]
    if not isinstance(action, dict) or action.get("name") != "silaoshi-base-notify":
        raise BridgeError("only the silaoshi-base-notify action is supported")
    command = action.get("command")
    if not isinstance(command, list) or not command or not all(isinstance(x, str) and x for x in command):
        raise BridgeError("action command must be a non-empty string list")
    timeout = action.get("timeout_seconds", 900)
    retries = action.get("max_retries", 2)
    if not isinstance(timeout, int) or not 1 <= timeout <= 900:
        raise BridgeError("action timeout_seconds must be between 1 and 900")
    if not isinstance(retries, int) or not 0 <= retries <= 3:
        raise BridgeError("action max_retries must be between 0 and 3")
    return cfg


def _secret(cfg: dict[str, Any]) -> bytes:
    path = Path(cfg["secret_file"])
    data = path.read_bytes().strip()
    if len(data) < 32:
        raise BridgeError("bridge secret is missing or too short")
    return data


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def _find_delivery_id(value: Any) -> str | None:
    if isinstance(value, dict):
        for key in DELIVERY_KEYS:
            candidate = value.get(key)
            if isinstance(candidate, str) and candidate.strip():
                return candidate.strip()
        for child in value.values():
            found = _find_delivery_id(child)
            if found:
                return found
    elif isinstance(value, list):
        for child in value:
            found = _find_delivery_id(child)
            if found:
                return found
    return None


def delivery_id(payload: dict[str, Any]) -> str:
    candidate = _find_delivery_id(payload)
    if candidate and len(candidate) <= 240:
        return candidate
    match = RECORD_ID_RE.search(json.dumps(payload, ensure_ascii=False))
    if match:
        return match.group(0)
    return "payload-sha256:" + hashlib.sha256(canonical_json(payload)).hexdigest()


def signed_envelope(payload: dict[str, Any], cfg: dict[str, Any], now: int | None = None) -> dict[str, Any]:
    body = {
        "schema_version": 1,
        "action": "silaoshi-base-notify",
        "delivery_id": delivery_id(payload),
        "timestamp": int(time.time() if now is None else now),
        "nonce": secrets.token_hex(16),
        "payload": payload,
    }
    signature = hmac.new(_secret(cfg), canonical_json(body), hashlib.sha256).hexdigest()
    return {**body, "signature": signature}


def verify_envelope(envelope: dict[str, Any], cfg: dict[str, Any], now: int | None = None) -> None:
    signature = envelope.get("signature")
    unsigned = {key: value for key, value in envelope.items() if key != "signature"}
    expected = hmac.new(_secret(cfg), canonical_json(unsigned), hashlib.sha256).hexdigest()
    if not isinstance(signature, str) or not hmac.compare_digest(signature, expected):
        raise BridgeError("signature is invalid")
    timestamp = envelope.get("timestamp")
    current = int(time.time() if now is None else now)
    if not isinstance(timestamp, int) or abs(current - timestamp) > 300:
        raise BridgeError("timestamp is outside the accepted window")
    if envelope.get("schema_version") != 1 or envelope.get("action") != "silaoshi-base-notify":
        raise BridgeError("envelope contract is invalid")
    if not isinstance(envelope.get("payload"), dict):
        raise BridgeError("payload must be an object")
    if not isinstance(envelope.get("delivery_id"), str) or not envelope["delivery_id"]:
        raise BridgeError("delivery_id is invalid")


def connect_db(cfg: dict[str, Any]) -> sqlite3.Connection:
    path = Path(cfg["database_path"])
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(path, timeout=10)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS jobs (
          delivery_id TEXT PRIMARY KEY,
          payload_json TEXT NOT NULL,
          status TEXT NOT NULL,
          attempts INTEGER NOT NULL DEFAULT 0,
          next_attempt_at INTEGER NOT NULL,
          created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL,
          returncode INTEGER,
          result_hash TEXT,
          notification_status TEXT
        )
        """
    )
    conn.execute(
        "UPDATE jobs SET status='retry',next_attempt_at=? WHERE status='running'",
        (int(time.time()),),
    )
    conn.commit()
    os.chmod(path, 0o600)
    return conn


def enqueue(conn: sqlite3.Connection, envelope: dict[str, Any], now: int | None = None) -> str:
    current = int(time.time() if now is None else now)
    cursor = conn.execute(
        "INSERT OR IGNORE INTO jobs "
        "(delivery_id,payload_json,status,next_attempt_at,created_at,updated_at) "
        "VALUES (?,?,?,?,?,?)",
        (envelope["delivery_id"], canonical_json(envelope["payload"]).decode(), "queued", current, current, current),
    )
    conn.commit()
    return "accepted" if cursor.rowcount == 1 else "duplicate"


def _validate_action(cfg: dict[str, Any]) -> list[str]:
    action = cfg["action"]
    command = list(action["command"])
    script = Path(command[1] if Path(command[0]).name.startswith("python") and len(command) > 1 else command[0])
    if not script.is_absolute() or not script.is_file():
        raise BridgeError("allowlisted action script is unavailable")
    expected = action.get("script_sha256")
    actual = hashlib.sha256(script.read_bytes()).hexdigest()
    if not isinstance(expected, str) or not hmac.compare_digest(actual, expected):
        raise BridgeError("allowlisted action script digest does not match")
    return command


def _notify(cfg: dict[str, Any], success: bool, stdout: str, stderr: str, returncode: int) -> None:
    action = cfg["action"]
    command = action.get("notification_command")
    if not command:
        return
    if not isinstance(command, list) or not all(isinstance(x, str) and x for x in command):
        raise BridgeError("notification command is invalid")
    template = action.get("success_template" if success else "failure_template", "")
    message = str(template).format(
        returncode=returncode,
        stdout=stdout[-2000:],
        stderr=stderr[-2000:],
    )
    subprocess.run(command, input=message, text=True, capture_output=True, timeout=180, check=True)


def run_one(conn: sqlite3.Connection, cfg: dict[str, Any], now: int | None = None) -> bool:
    current = int(time.time() if now is None else now)
    row = conn.execute(
        "SELECT delivery_id,payload_json,attempts FROM jobs "
        "WHERE status IN ('queued','retry') AND next_attempt_at<=? "
        "ORDER BY created_at LIMIT 1",
        (current,),
    ).fetchone()
    if row is None:
        return False
    delivery, payload_json, attempts = row
    conn.execute(
        "UPDATE jobs SET status='running',attempts=attempts+1,updated_at=? WHERE delivery_id=?",
        (current, delivery),
    )
    conn.commit()
    stdout = ""
    stderr = ""
    returncode = -1
    try:
        proc = subprocess.run(
            _validate_action(cfg),
            input=payload_json,
            text=True,
            capture_output=True,
            timeout=cfg["action"].get("timeout_seconds", 900),
            check=False,
        )
        stdout, stderr, returncode = proc.stdout or "", proc.stderr or "", proc.returncode
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout or ""
        stderr = "script timed out"
    except (BridgeError, OSError) as exc:
        stderr = str(exc)
    success = returncode == 0
    result_hash = hashlib.sha256((stdout + "\0" + stderr).encode()).hexdigest()
    max_retries = cfg["action"].get("max_retries", 2)
    if success:
        status, next_attempt = "succeeded", current
    elif attempts < max_retries:
        status, next_attempt = "retry", current + min(300, 30 * (2**attempts))
    else:
        status, next_attempt = "failed", current
    conn.execute(
        "UPDATE jobs SET status=?,next_attempt_at=?,updated_at=?,returncode=?,result_hash=? WHERE delivery_id=?",
        (status, next_attempt, current, returncode, result_hash, delivery),
    )
    conn.commit()
    notification_status = "not_due"
    if status in {"succeeded", "failed"}:
        try:
            _notify(cfg, success, stdout, stderr, returncode)
            notification_status = "sent" if cfg["action"].get("notification_command") else "disabled"
        except (BridgeError, OSError, subprocess.SubprocessError):
            notification_status = "failed"
        conn.execute(
            "UPDATE jobs SET notification_status=?,updated_at=? WHERE delivery_id=?",
            (notification_status, current, delivery),
        )
        conn.commit()
    print(json.dumps({"delivery_hash": hashlib.sha256(delivery.encode()).hexdigest(), "status": status, "attempt": attempts + 1, "returncode": returncode, "notification_status": notification_status}, sort_keys=True))
    return True


def transform(cfg: dict[str, Any]) -> int:
    raw = sys.stdin.buffer.read(MAX_PAYLOAD_BYTES + 1)
    if not raw or len(raw) > MAX_PAYLOAD_BYTES:
        raise BridgeError("payload length is invalid")
    payload = json.loads(raw)
    if not isinstance(payload, dict):
        raise BridgeError("payload must be an object")
    request = canonical_json(signed_envelope(payload, cfg)) + b"\n"
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(3)
        client.connect(cfg["socket_path"])
        client.sendall(request)
        response = client.recv(MAX_RESPONSE_BYTES)
    result = json.loads(response)
    if result.get("status") not in {"accepted", "duplicate"}:
        raise BridgeError("runner rejected the job")
    print("[SILENT]")
    return 0


def serve(cfg: dict[str, Any]) -> int:
    socket_path = Path(cfg["socket_path"])
    socket_path.parent.mkdir(parents=True, exist_ok=True)
    if socket_path.exists():
        if not socket_path.is_socket():
            raise BridgeError("socket path exists and is not a socket")
        socket_path.unlink()
    conn = connect_db(cfg)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
        server.bind(str(socket_path))
        os.chmod(socket_path, 0o600)
        server.listen(16)
        server.settimeout(1)
        try:
            while True:
                try:
                    client, _ = server.accept()
                except TimeoutError:
                    run_one(conn, cfg)
                    continue
                with client:
                    raw = b""
                    while not raw.endswith(b"\n") and len(raw) <= MAX_PAYLOAD_BYTES:
                        chunk = client.recv(8192)
                        if not chunk:
                            break
                        raw += chunk
                    try:
                        envelope = json.loads(raw)
                        verify_envelope(envelope, cfg)
                        status = enqueue(conn, envelope)
                        response = {"status": status}
                    except Exception:
                        response = {"status": "rejected"}
                    client.sendall(canonical_json(response) + b"\n")
                run_one(conn, cfg)
        finally:
            socket_path.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("transform", "serve", "run-once"))
    parser.add_argument("--config", default=os.environ.get("QINTOPIA_SILAOSHI_SCRIPT_ACTION_CONFIG", DEFAULT_CONFIG))
    args = parser.parse_args()
    cfg = load_config(Path(args.config))
    if args.mode == "transform":
        return transform(cfg)
    if args.mode == "serve":
        return serve(cfg)
    conn = connect_db(cfg)
    return 0 if run_one(conn, cfg) else 3


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (BridgeError, OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"silaoshi script action bridge: {exc}", file=sys.stderr)
        raise SystemExit(1)
