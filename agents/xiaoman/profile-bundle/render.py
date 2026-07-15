#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent
MANIFEST_PATH = ROOT / "bundle.json"
PLACEHOLDER = re.compile(r"\{\{([A-Z][A-Z0-9_]*)\}\}")
WE_COM_TARGET = re.compile(r"[A-Za-z0-9._:@-]+")
HEX_SHA256 = re.compile(r"[0-9a-f]{64}")
REQUIRED_EXCLUSIONS = {
    ".env",
    "config.yaml",
    "webhook_subscriptions.json",
    "channel_directory.json",
    "cron/jobs.json",
    "sessions",
    "auth",
    "messages",
    "memories",
    "logs",
    "cache",
    "locks",
    "state.db",
}
MAX_JSON_BYTES = 65_536


class BundleError(ValueError):
    pass


def load_json(path: Path) -> dict[str, Any]:
    try:
        data = path.read_bytes()
        if len(data) > MAX_JSON_BYTES:
            raise BundleError(f"JSON file exceeds the size limit: {path}")
        value = json.loads(data.decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise BundleError(f"cannot read JSON file: {path}") from exc
    if not isinstance(value, dict):
        raise BundleError(f"JSON root must be an object: {path}")
    return value


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def validate_manifest(
    manifest: dict[str, Any],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    if manifest.get("schema_version") != 1:
        raise BundleError("bundle.json schema_version must be 1")
    if manifest.get("agent_id") != "xiaoman":
        raise BundleError("bundle.json agent_id must be xiaoman")
    if manifest.get("status") != "observation-only":
        raise BundleError("bundle.json status must remain observation-only")

    inputs = manifest.get("inputs")
    files = manifest.get("files")
    if not isinstance(inputs, list) or not inputs:
        raise BundleError("bundle.json inputs must be a non-empty array")
    if not isinstance(files, list) or not files:
        raise BundleError("bundle.json files must be a non-empty array")

    input_names: set[str] = set()
    for item in inputs:
        if not isinstance(item, dict):
            raise BundleError("bundle.json input entries must be objects")
        name = item.get("name")
        kind = item.get("kind")
        max_length = item.get("max_length")
        if not isinstance(name, str) or not re.fullmatch(
            r"QINTOPIA_XIAOMAN_[A-Z0-9_]+", name
        ):
            raise BundleError("bundle.json input name is invalid")
        if name in input_names:
            raise BundleError(f"bundle.json input is duplicated: {name}")
        if kind not in {"display_name", "wecom_target"}:
            raise BundleError(f"bundle.json input kind is invalid: {name}")
        if not isinstance(max_length, int) or max_length < 1 or max_length > 256:
            raise BundleError(f"bundle.json input max_length is invalid: {name}")
        if set(item) != {"name", "kind", "max_length"}:
            raise BundleError(f"bundle.json input contains unsupported fields: {name}")
        input_names.add(name)

    targets: set[str] = set()
    discovered_placeholders: set[str] = set()
    for item in files:
        if not isinstance(item, dict):
            raise BundleError("bundle.json file entries must be objects")
        template = item.get("template")
        target = item.get("target")
        mode = item.get("mode")
        source_sha = item.get("production_source_sha256")
        if not isinstance(template, str) or not template.startswith("templates/"):
            raise BundleError("bundle.json template path must stay under templates/")
        template_path = (ROOT / template).resolve()
        if ROOT not in template_path.parents or not template_path.is_file():
            raise BundleError(f"bundle template is missing: {template}")
        if not isinstance(target, str) or Path(target).name != target:
            raise BundleError(f"bundle target must be a file name: {target}")
        if target in targets:
            raise BundleError(f"bundle target is duplicated: {target}")
        if mode not in {"0600", "0644"}:
            raise BundleError(f"bundle mode is not allowlisted: {target}")
        if not isinstance(source_sha, str) or not HEX_SHA256.fullmatch(source_sha):
            raise BundleError(f"bundle production source hash is invalid: {target}")
        if set(item) != {"template", "target", "mode", "production_source_sha256"}:
            raise BundleError(f"bundle file contains unsupported fields: {target}")
        template_text = template_path.read_text(encoding="utf-8")
        discovered_placeholders.update(PLACEHOLDER.findall(template_text))
        targets.add(target)

    unknown = discovered_placeholders - input_names
    unused = input_names - discovered_placeholders
    if unknown:
        raise BundleError(f"template contains undeclared input: {sorted(unknown)[0]}")
    if unused:
        raise BundleError(f"bundle input is unused: {sorted(unused)[0]}")

    exclusions = manifest.get("excluded_runtime_state")
    if not isinstance(exclusions, list) or not REQUIRED_EXCLUSIONS.issubset(
        set(exclusions)
    ):
        raise BundleError("bundle.json must retain every required runtime-state exclusion")

    boundary = manifest.get("production_boundary")
    boundary_keys = (
        "live_profile_changes",
        "external_sends",
        "database_writes",
        "network_access",
    )
    if not isinstance(boundary, dict) or any(
        boundary.get(key) is not False for key in boundary_keys
    ):
        raise BundleError("bundle.json production boundary must remain observation-only")

    return inputs, files


def validate_value(item: dict[str, Any], value: Any) -> str:
    name = item["name"]
    if not isinstance(value, str) or not value:
        raise BundleError(f"profile input must be a non-empty string: {name}")
    if len(value) > item["max_length"]:
        raise BundleError(f"profile input exceeds its maximum length: {name}")
    if any(ord(char) < 32 or ord(char) == 127 for char in value):
        raise BundleError(f"profile input contains a control character: {name}")
    if "{{" in value or "}}" in value:
        raise BundleError(f"profile input contains template delimiters: {name}")
    if item["kind"] == "wecom_target" and not WE_COM_TARGET.fullmatch(value):
        raise BundleError(f"profile input is not a valid WeCom target: {name}")
    return value


def render(values_path: Path, output_dir: Path) -> None:
    manifest = load_json(MANIFEST_PATH)
    inputs, files = validate_manifest(manifest)
    values = load_json(values_path)
    declared_names = {item["name"] for item in inputs}
    missing = declared_names - set(values)
    extra = set(values) - declared_names
    if missing:
        raise BundleError(f"profile input is missing: {sorted(missing)[0]}")
    if extra:
        raise BundleError(f"profile input is not allowlisted: {sorted(extra)[0]}")
    validated = {
        item["name"]: validate_value(item, values[item["name"]]) for item in inputs
    }

    if output_dir.exists():
        raise BundleError(f"output directory already exists: {output_dir}")
    output_dir.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(
        tempfile.mkdtemp(prefix=f".{output_dir.name}-", dir=output_dir.parent)
    )
    rendered_files = []
    try:
        for item in files:
            source = ROOT / item["template"]
            text = source.read_text(encoding="utf-8")
            rendered = PLACEHOLDER.sub(lambda match: validated[match.group(1)], text)
            if PLACEHOLDER.search(rendered):
                raise BundleError(f"rendered file contains unresolved input: {item['target']}")
            data = rendered.encode("utf-8")
            target = temporary / item["target"]
            target.write_bytes(data)
            os.chmod(target, int(item["mode"], 8))
            rendered_files.append({
                "path": item["target"],
                "mode": item["mode"],
                "sha256": sha256(data),
                "size_bytes": len(data),
                "production_source_sha256": item["production_source_sha256"],
            })

        output_manifest = {
            "schema_version": 1,
            "agent_id": "xiaoman",
            "status": "observation-only",
            "input_names": [item["name"] for item in inputs],
            "files": rendered_files,
            "production_boundary": manifest["production_boundary"],
        }
        manifest_target = temporary / "bundle-manifest.json"
        manifest_target.write_text(
            json.dumps(output_manifest, ensure_ascii=True, indent=2) + "\n",
            encoding="utf-8",
        )
        os.chmod(manifest_target, 0o600)
        temporary.rename(output_dir)
    except Exception:
        shutil.rmtree(temporary, ignore_errors=True)
        raise


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Render the reviewed Xiaoman profile bundle"
    )
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--values-file", type=Path)
    parser.add_argument("--output-dir", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        manifest = load_json(MANIFEST_PATH)
        validate_manifest(manifest)
        if args.check_only:
            if args.values_file or args.output_dir:
                raise BundleError("--check-only does not accept render arguments")
            print("Xiaoman profile bundle check passed.")
            return 0
        if args.values_file is None or args.output_dir is None:
            raise BundleError("--values-file and --output-dir are required for rendering")
        render(args.values_file.resolve(), args.output_dir.resolve())
        print("Xiaoman profile bundle rendered.")
        return 0
    except BundleError as exc:
        print(f"Xiaoman profile bundle error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
