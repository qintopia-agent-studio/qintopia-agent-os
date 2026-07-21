#!/usr/bin/env python3
"""Prepare Erhua's server-local Livecool binding without disclosing its value."""

from __future__ import annotations

import argparse
import json
import os
import re
import tempfile
from pathlib import Path

from render_profile_overlay import (
    digest_bytes,
    load_yaml,
    reject_output_alias,
    require_regular_input,
)


BINDING = "LIVECOOL_API_KEY"
BINDING_RE = re.compile(r"^\s*(?:export\s+)?LIVECOOL_API_KEY\s*=\s*(.*)$")


def env_bindings(text: str) -> list[str]:
    return [match.group(1).strip() for line in text.splitlines() if (match := BINDING_RE.match(line))]


def nonempty_binding(value: str) -> bool:
    if not value:
        return False
    if value[:1] in {"'", '"'}:
        return len(value) >= 2 and value[-1] == value[0] and bool(value[1:-1].strip())
    return bool(value.split("#", 1)[0].strip())


def binding_value(value: str) -> str:
    value = value.strip()
    if value[:1] == "'" and value[-1:] == "'":
        return value[1:-1]
    if value[:1] == '"' and value[-1:] == '"':
        body = value[1:-1]
        return body.replace(r"\"", '"').replace(r"\\", "\\")
    return value.split("#", 1)[0].strip()


def quote_env(value: str) -> str:
    if "\n" in value or "\r" in value or "\x00" in value:
        raise ValueError("credential contains unsupported control characters")
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def atomic_write(path: Path, text: str) -> None:
    reject_output_alias(path, [])
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def source_credential(default_config: Path) -> str:
    config = load_yaml(default_config)
    providers = config.get("custom_providers") if isinstance(config, dict) else None
    if not isinstance(providers, list):
        raise ValueError("default config custom_providers must be a list")
    matches = [
        provider
        for provider in providers
        if isinstance(provider, dict)
        and str(provider.get("name", "")).strip().lower() == "livecool.net"
    ]
    if len(matches) != 1:
        raise ValueError("default config must contain exactly one Livecool.net provider")
    credential = matches[0].get("api_key")
    if not isinstance(credential, str) or not credential.strip():
        raise ValueError("default Livecool.net inline credential is missing")
    return credential


def prepare(args: argparse.Namespace) -> None:
    env_path = Path(args.env)
    default_config = Path(args.default_config)
    output_path = Path(args.output)
    report_path = Path(args.report)
    require_regular_input(env_path)
    require_regular_input(default_config)
    reject_output_alias(output_path, [env_path, default_config])
    reject_output_alias(report_path, [env_path, default_config])
    if output_path.resolve(strict=False) == report_path.resolve(strict=False):
        raise ValueError("output and report paths must be distinct")

    text = env_path.read_text(encoding="utf-8")
    before_bytes = text.encode("utf-8")
    bindings = env_bindings(text)
    if len(bindings) > 1:
        raise ValueError(f"{BINDING} is duplicated in Erhua .env")
    if bindings and not nonempty_binding(bindings[0]):
        raise ValueError(f"{BINDING} is empty in Erhua .env")

    credential = source_credential(default_config)
    status = "existing"
    if not bindings:
        suffix = "" if not text or text.endswith("\n") else "\n"
        text = f"{text}{suffix}{BINDING}={quote_env(credential)}\n"
        status = "migrated"
    elif binding_value(bindings[0]) != credential:
        raise ValueError(f"{BINDING} conflicts with the approved Livecool credential")

    report = {
        "schema_version": 1,
        "agent_id": "erhua",
        "binding": BINDING,
        "status": status,
        "before_sha256": digest_bytes(before_bytes),
        "after_sha256": digest_bytes(text.encode("utf-8")),
        "output_mode": "0600",
        "secret_values_redacted": True,
    }
    atomic_write(output_path, text)
    atomic_write(report_path, json.dumps(report, indent=2) + "\n")


def check(args: argparse.Namespace) -> None:
    env_path = Path(args.env)
    require_regular_input(env_path)
    bindings = env_bindings(env_path.read_text(encoding="utf-8"))
    if len(bindings) != 1 or not nonempty_binding(bindings[0]):
        raise ValueError(f"{BINDING} must appear exactly once and be non-empty")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("--env", required=True)
    prepare_parser.add_argument("--default-config", required=True)
    prepare_parser.add_argument("--output", required=True)
    prepare_parser.add_argument("--report", required=True)
    prepare_parser.set_defaults(func=prepare)
    check_parser = subparsers.add_parser("check")
    check_parser.add_argument("--env", required=True)
    check_parser.set_defaults(func=check)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        args.func(args)
    except (OSError, UnicodeError, ValueError) as exc:
        print(f"Livecool environment migration failed: {exc}", file=os.sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
