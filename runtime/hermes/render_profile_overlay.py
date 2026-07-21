#!/usr/bin/env python3
"""Render and verify the fixed, non-secret Erhua model overlay."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import stat
import tempfile
from pathlib import Path
from typing import Any

import yaml
from yaml.events import AliasEvent


MANAGED_MODEL_KEYS = ("default", "provider", "base_url")
PROVIDER_KEYS = ("name", "base_url", "model", "key_env", "api_mode")
PRESERVED_PROVIDER_FIELDS = {"timeout"}
FORBIDDEN_PROVIDER_FIELDS = {
    "api_key",
    "api_key_env",
    "authorization",
    "headers",
    "token",
    "bearer_token",
    "secret",
    "credential",
    "password",
    "api_secret",
    "access_token",
    "refresh_token",
}
EXPECTED_OVERLAY = {
    "profile_overlay_version": 1,
    "agent_id": "erhua",
    "managed": {
        "model": {
            "default": "gpt-5.5",
            "provider": "custom:livecool.net",
            "base_url": "",
        },
        "custom_provider": {
            "name": "Livecool.net",
            "base_url": "https://livecool.net/v1",
            "model": "gpt-5.5",
            "key_env": "LIVECOOL_API_KEY",
            "api_mode": "chat_completions",
        },
    },
}


class StrictLoader(yaml.SafeLoader):
    """Safe YAML loader that rejects aliases and duplicate mapping keys."""

    def compose_node(self, parent: Any, index: Any) -> Any:
        if self.check_event(AliasEvent):
            raise yaml.constructor.ConstructorError(
                None, None, "YAML aliases are not allowed", self.peek_event().start_mark
            )
        return super().compose_node(parent, index)

    def construct_mapping(self, node: Any, deep: bool = False) -> dict[Any, Any]:
        mapping: dict[Any, Any] = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            try:
                duplicate = key in mapping
            except TypeError as exc:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    "found an unhashable key",
                    key_node.start_mark,
                ) from exc
            if duplicate:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key {key!r}",
                    key_node.start_mark,
                )
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


def load_yaml(path: Path) -> Any:
    require_regular_input(path)
    try:
        return yaml.load(path.read_text(encoding="utf-8"), Loader=StrictLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        raise ValueError(f"cannot load YAML from {path}: {exc}") from exc


def require_regular_input(path: Path) -> None:
    if path.is_symlink():
        raise ValueError(f"symlinked input is not allowed: {path}")
    try:
        mode = path.stat().st_mode
    except OSError as exc:
        raise ValueError(f"input is unavailable: {path}") from exc
    if not stat.S_ISREG(mode):
        raise ValueError(f"input must be a regular file: {path}")


def reject_output_alias(path: Path, inputs: list[Path]) -> None:
    if path.is_symlink():
        raise ValueError(f"symlinked output is not allowed: {path}")
    parent = path.parent
    if parent.is_symlink() or not parent.is_dir():
        raise ValueError(f"output parent must be a real directory: {parent}")
    if path.exists():
        output_stat = path.stat()
        for input_path in inputs:
            input_stat = input_path.stat()
            if (output_stat.st_dev, output_stat.st_ino) == (
                input_stat.st_dev,
                input_stat.st_ino,
            ):
                raise ValueError("output must not alias an input file")


def require_exact_mapping(value: Any, keys: tuple[str, ...], label: str) -> None:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a mapping")
    if set(value) != set(keys):
        raise ValueError(f"{label} fields must be exactly: {', '.join(keys)}")


def validate_overlay(overlay: Any) -> dict[str, Any]:
    require_exact_mapping(
        overlay,
        ("profile_overlay_version", "agent_id", "managed"),
        "overlay",
    )
    require_exact_mapping(overlay["managed"], ("model", "custom_provider"), "managed")
    require_exact_mapping(overlay["managed"]["model"], MANAGED_MODEL_KEYS, "model")
    require_exact_mapping(
        overlay["managed"]["custom_provider"], PROVIDER_KEYS, "custom_provider"
    )
    if overlay != EXPECTED_OVERLAY:
        raise ValueError("overlay does not match the approved Erhua Livecool contract")
    return overlay


def render(base: Any, overlay: dict[str, Any]) -> tuple[dict[str, Any], list[str]]:
    if not isinstance(base, dict):
        raise ValueError("base config must be a mapping")
    candidate = copy.deepcopy(base)
    model = candidate.get("model")
    if not isinstance(model, dict):
        raise ValueError("base config model must be a mapping")
    providers = candidate.get("custom_providers")
    if providers is None:
        providers = []
        candidate["custom_providers"] = providers
    if not isinstance(providers, list):
        raise ValueError("base config custom_providers must be a list")

    provider_indexes: dict[str, int] = {}
    for index, provider in enumerate(providers):
        if not isinstance(provider, dict):
            raise ValueError(f"custom_providers[{index}] must be a mapping")
        name = provider.get("name")
        if not isinstance(name, str) or not name.strip():
            raise ValueError(f"custom_providers[{index}].name must be a non-empty string")
        normalized = name.strip().lower()
        if normalized in provider_indexes:
            raise ValueError(f"duplicate custom provider name: {name}")
        provider_indexes[normalized] = index

    desired_model = overlay["managed"]["model"]
    desired_provider = overlay["managed"]["custom_provider"]
    changed: list[str] = []
    for key in MANAGED_MODEL_KEYS:
        if model.get(key) != desired_model[key]:
            changed.append(f"model.{key}")
        model[key] = desired_model[key]

    provider_name = desired_provider["name"].lower()
    if provider_name in provider_indexes:
        index = provider_indexes[provider_name]
        current_provider = providers[index]
        rendered_provider = copy.deepcopy(desired_provider)
        for key in PRESERVED_PROVIDER_FIELDS:
            if key in current_provider and key not in rendered_provider:
                rendered_provider[key] = current_provider[key]
        if current_provider != rendered_provider:
            changed.append(f"custom_providers[name={desired_provider['name']}]")
        providers[index] = rendered_provider
    else:
        providers.append(copy.deepcopy(desired_provider))
        changed.append(f"custom_providers[name={desired_provider['name']}]")

    return candidate, changed


def digest_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def yaml_bytes(value: Any) -> bytes:
    return yaml.safe_dump(value, sort_keys=False, allow_unicode=True).encode("utf-8")


def atomic_write(path: Path, data: bytes, mode: int = 0o600) -> None:
    reject_output_alias(path, [])
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(fd, mode)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def render_command(args: argparse.Namespace) -> None:
    base_path = Path(args.base)
    overlay_path = Path(args.overlay)
    output_path = Path(args.output)
    report_path = Path(args.report)
    require_regular_input(base_path)
    require_regular_input(overlay_path)
    reject_output_alias(output_path, [base_path, overlay_path])
    reject_output_alias(report_path, [base_path, overlay_path])
    if output_path.resolve(strict=False) == report_path.resolve(strict=False):
        raise ValueError("output and report paths must be distinct")

    base_bytes = base_path.read_bytes()
    base = load_yaml(base_path)
    overlay = validate_overlay(load_yaml(overlay_path))
    candidate, changed = render(base, overlay)
    candidate_bytes = yaml_bytes(candidate)
    report = {
        "schema_version": 1,
        "agent_id": "erhua",
        "status": "unchanged" if not changed else "changed",
        "changed_paths": changed,
        "before_sha256": digest_bytes(base_bytes),
        "after_sha256": digest_bytes(candidate_bytes),
        "output_mode": "0600",
        "secret_values_redacted": True,
    }
    atomic_write(output_path, candidate_bytes)
    atomic_write(report_path, (json.dumps(report, indent=2) + "\n").encode("utf-8"))


def verify_command(args: argparse.Namespace) -> None:
    config_path = Path(args.config)
    overlay_path = Path(args.overlay)
    config = load_yaml(config_path)
    overlay = validate_overlay(load_yaml(overlay_path))
    candidate, changed = render(config, overlay)
    if changed or candidate != config:
        raise ValueError("rendered config does not satisfy the approved overlay")
    provider = next(
        item
        for item in candidate["custom_providers"]
        if item.get("name", "").lower() == "livecool.net"
    )
    if "api_key" in provider or "api_key_env" in provider:
        raise ValueError("rendered provider must not contain an inline credential")
    forbidden = set(provider) & FORBIDDEN_PROVIDER_FIELDS
    if forbidden:
        raise ValueError(
            f"rendered provider contains forbidden fields: {', '.join(sorted(forbidden))}"
        )
    extra = set(provider) - set(PROVIDER_KEYS) - PRESERVED_PROVIDER_FIELDS
    if extra:
        raise ValueError(
            f"rendered provider contains unapproved fields: {', '.join(sorted(extra))}"
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    render_parser = subparsers.add_parser("render")
    render_parser.add_argument("--base", required=True)
    render_parser.add_argument("--overlay", required=True)
    render_parser.add_argument("--output", required=True)
    render_parser.add_argument("--report", required=True)
    render_parser.set_defaults(func=render_command)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--config", required=True)
    verify_parser.add_argument("--overlay", required=True)
    verify_parser.set_defaults(func=verify_command)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        args.func(args)
    except (OSError, ValueError, yaml.YAMLError) as exc:
        print(f"profile overlay failed: {exc}", file=os.sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
