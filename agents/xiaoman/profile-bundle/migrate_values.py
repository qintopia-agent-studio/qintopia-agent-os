#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import sys
import tempfile
from pathlib import Path
from typing import Any

import render as bundle_renderer


BUNDLE_ROOT = Path(__file__).resolve().parent
LIVE_SOUL_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/SOUL.md")
LIVE_PROFILE_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/profile.yaml")
OUTPUT_PATH = Path("/etc/qintopia/xiaoman-profile-bundle-values.json")
APPROVAL_ENV = "QINTOPIA_XIAOMAN_PROFILE_VALUES_MIGRATION_APPROVAL"
APPROVAL_PHRASE = "approved-xiaoman-profile-values-migration"
MAX_SOURCE_BYTES = 65_536
VALUE_PATTERNS = {
    "QINTOPIA_XIAOMAN_OPERATIONS_OWNER_NAME": re.compile(
        r"当前生产对接碳基人是企业微信上的([^。]+)。"
    ),
    "QINTOPIA_XIAOMAN_OPERATIONS_OWNER_WECOM_TARGET": re.compile(
        r"日志中的 `user` 和 `chat` 均为 `([^`]+)`"
    ),
    "QINTOPIA_XIAOMAN_TECHNICAL_OWNER_NAME": re.compile(
        r"当前处于从(.+?)设计/研发交接给"
    ),
    "QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL": re.compile(
        r"`WECOM_HOME_CHANNEL` 从 `([^`]+)` 改成"
    ),
}


class MigrationError(ValueError):
    pass


def read_regular_bytes(path: Path, maximum_bytes: int) -> bytes:
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise MigrationError(f"required regular file is unavailable: {path}") from exc
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise MigrationError(f"required path is not a regular file: {path}")
        chunks = []
        total = 0
        while True:
            chunk = os.read(descriptor, min(16_384, maximum_bytes + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > maximum_bytes:
                raise MigrationError(f"required file exceeds the size limit: {path}")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def decode_utf8(data: bytes, label: str) -> str:
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise MigrationError(f"{label} is not valid UTF-8") from exc


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def extract_values(soul_text: str) -> dict[str, str]:
    values = {}
    for name, pattern in VALUE_PATTERNS.items():
        matches = pattern.findall(soul_text)
        if len(matches) != 1:
            raise MigrationError(f"live SOUL value shape mismatch: {name}")
        values[name] = matches[0]
    return values


def validate_output_parent(path: Path, require_root_parent: bool) -> None:
    try:
        metadata = path.lstat()
    except OSError as exc:
        raise MigrationError("values output parent is unavailable") from exc
    if not stat.S_ISDIR(metadata.st_mode) or path.is_symlink():
        raise MigrationError("values output parent must be a regular directory")
    if require_root_parent and metadata.st_uid != 0:
        raise MigrationError("values output parent must be root-owned")
    if require_root_parent and stat.S_IMODE(metadata.st_mode) & 0o022:
        raise MigrationError("values output parent must not be group or world writable")


def write_no_clobber(path: Path, data: bytes) -> None:
    temporary_descriptor = -1
    temporary_path: Path | None = None
    try:
        temporary_descriptor, temporary_name = tempfile.mkstemp(
            prefix=f".{path.name}-", dir=path.parent
        )
        temporary_path = Path(temporary_name)
        os.fchmod(temporary_descriptor, 0o600)
        with os.fdopen(temporary_descriptor, "wb", closefd=True) as handle:
            temporary_descriptor = -1
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        try:
            os.link(temporary_path, path, follow_symlinks=False)
        except FileExistsError as exc:
            raise MigrationError("values output already exists") from exc
        temporary_path.unlink()
        temporary_path = None
        directory_flags = os.O_RDONLY
        if hasattr(os, "O_DIRECTORY"):
            directory_flags |= os.O_DIRECTORY
        if hasattr(os, "O_NOFOLLOW"):
            directory_flags |= os.O_NOFOLLOW
        directory_descriptor = os.open(path.parent, directory_flags)
        try:
            os.fsync(directory_descriptor)
        finally:
            os.close(directory_descriptor)
    except MigrationError:
        raise
    except OSError as exc:
        raise MigrationError("values output could not be created") from exc
    finally:
        if temporary_descriptor >= 0:
            os.close(temporary_descriptor)
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def migrate_values(
    *,
    bundle_root: Path,
    live_soul_path: Path,
    live_profile_path: Path,
    output_path: Path,
    approval: str,
    effective_uid: int,
    require_root_parent: bool,
) -> dict[str, Any]:
    if approval != APPROVAL_PHRASE:
        raise MigrationError(f"exact {APPROVAL_ENV} approval is required")
    if effective_uid != 0:
        raise MigrationError("Xiaoman profile values migration must run as root")

    validate_output_parent(output_path.parent, require_root_parent)
    if os.path.lexists(output_path):
        raise MigrationError("values output already exists")

    manifest = bundle_renderer.load_json(bundle_root / "bundle.json")
    inputs, files = bundle_renderer.validate_manifest(manifest, bundle_root)
    files_by_target = {item["target"]: item for item in files}
    if set(files_by_target) != {"SOUL.md", "profile.yaml"}:
        raise MigrationError("profile bundle file allowlist mismatch")

    live_soul = read_regular_bytes(live_soul_path, MAX_SOURCE_BYTES)
    live_profile = read_regular_bytes(live_profile_path, MAX_SOURCE_BYTES)
    live_hashes = {
        "SOUL.md": sha256(live_soul),
        "profile.yaml": sha256(live_profile),
    }
    for target, live_hash in live_hashes.items():
        if live_hash != files_by_target[target]["production_source_sha256"]:
            raise MigrationError(f"reviewed production source hash mismatch: {target}")

    values = extract_values(decode_utf8(live_soul, "live SOUL.md"))
    inputs_by_name = {item["name"]: item for item in inputs}
    if set(values) != set(inputs_by_name):
        raise MigrationError("extracted profile input allowlist mismatch")
    validated_values = {
        name: bundle_renderer.validate_value(inputs_by_name[name], value)
        for name, value in values.items()
    }
    serialized_values = (
        json.dumps(validated_values, ensure_ascii=True, indent=2) + "\n"
    ).encode("utf-8")

    temporary_root = Path(
        tempfile.mkdtemp(prefix=".xiaoman-profile-values-parity-", dir=output_path.parent)
    )
    try:
        temporary_values = temporary_root / "values.json"
        temporary_values.write_bytes(serialized_values)
        os.chmod(temporary_values, 0o600)
        rendered_dir = temporary_root / "rendered"
        bundle_renderer.render(temporary_values, rendered_dir, bundle_root)
        if (rendered_dir / "SOUL.md").read_bytes() != live_soul:
            raise MigrationError("rendered SOUL.md parity mismatch")
        if (rendered_dir / "profile.yaml").read_bytes() != live_profile:
            raise MigrationError("rendered profile.yaml parity mismatch")
    finally:
        shutil.rmtree(temporary_root, ignore_errors=True)

    if read_regular_bytes(live_soul_path, MAX_SOURCE_BYTES) != live_soul:
        raise MigrationError("live SOUL.md changed during values migration")
    if read_regular_bytes(live_profile_path, MAX_SOURCE_BYTES) != live_profile:
        raise MigrationError("live profile.yaml changed during values migration")

    write_no_clobber(output_path, serialized_values)
    return {
        "schema_version": 1,
        "status": "xiaoman_profile_values_migration_succeeded",
        "output_created": True,
        "output_mode": "0600",
        "input_names": [item["name"] for item in inputs],
        "source_hashes": live_hashes,
        "live_profile_modified": False,
        "symlink_created": False,
        "network_accessed": False,
        "external_send_executed": False,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Prepare Xiaoman server-local profile bundle values"
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check-only", action="store_true")
    mode.add_argument("--apply", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.check_only:
            manifest = bundle_renderer.load_json(BUNDLE_ROOT / "bundle.json")
            bundle_renderer.validate_manifest(manifest, BUNDLE_ROOT)
            if set(VALUE_PATTERNS) != {item["name"] for item in manifest["inputs"]}:
                raise MigrationError("migration input allowlist mismatch")
            print("Xiaoman profile values migration check passed.")
            return 0
        report = migrate_values(
            bundle_root=BUNDLE_ROOT,
            live_soul_path=LIVE_SOUL_PATH,
            live_profile_path=LIVE_PROFILE_PATH,
            output_path=OUTPUT_PATH,
            approval=os.environ.get(APPROVAL_ENV, ""),
            effective_uid=os.geteuid(),
            require_root_parent=True,
        )
        print(json.dumps(report, separators=(",", ":")))
        return 0
    except (MigrationError, bundle_renderer.BundleError) as exc:
        print(f"Xiaoman profile values migration error: {exc}", file=sys.stderr)
        return 1
    except Exception:
        print("Xiaoman profile values migration error: unexpected failure", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
