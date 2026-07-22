#!/usr/bin/env python3
"""Validate the bounded Python entry used for Hermes provider resolution."""

from __future__ import annotations

import argparse
import os
import re
from pathlib import Path


def absolute_path(value: str, name: str) -> Path:
    path = Path(value)
    normalized = Path(os.path.abspath(value))
    if not path.is_absolute() or path != normalized:
        raise ValueError(f"{name} must be an absolute normalized path")
    return path


def require_unaliased_directory(path: Path, name: str) -> None:
    if not path.is_dir() or path.is_symlink() or path.resolve() != path:
        raise ValueError(f"{name} must be an existing non-aliased directory")


def require_executable_entry(entry: Path) -> Path:
    if not entry.exists() or not entry.is_file() or not os.access(entry, os.X_OK):
        raise ValueError("Hermes Python entry must be an executable file")
    try:
        resolved = entry.resolve(strict=True)
    except OSError as exc:
        raise ValueError("Hermes Python entry target cannot be resolved") from exc
    if not resolved.is_file() or resolved.is_symlink():
        raise ValueError("Hermes Python entry target must be a regular file")
    return resolved


def read_venv_home(config: Path) -> Path:
    fields: dict[str, str] = {}
    lines = config.read_text(encoding="utf-8").splitlines()
    for line_number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        key, separator, value = stripped.partition("=")
        key = key.strip().lower()
        value = value.strip()
        if not separator or not key or not value:
            raise ValueError(f"Hermes venv pyvenv.cfg line {line_number} is invalid")
        if key in fields:
            raise ValueError(f"Hermes venv pyvenv.cfg contains duplicate {key}")
        fields[key] = value
    home = fields.get("home", "")
    if not home:
        raise ValueError("Hermes venv pyvenv.cfg must declare home")
    return absolute_path(home, "Hermes venv base interpreter home")


def validate_venv_entry_target(
    resolved_entry: Path, venv_dir: Path, pyvenv_config: Path
) -> None:
    try:
        resolved_entry.relative_to(venv_dir)
        return
    except ValueError:
        pass

    base_home = read_venv_home(pyvenv_config)
    require_unaliased_directory(base_home, "Hermes venv base interpreter home")
    if resolved_entry.parent != base_home or not re.fullmatch(
        r"python(?:3(?:\.[0-9]+)?)?", resolved_entry.name
    ):
        raise ValueError(
            "Hermes venv Python target does not match pyvenv.cfg home"
        )


def validate(python_entry: Path, venv_dir: Path, release_dir: Path) -> str:
    require_unaliased_directory(release_dir, "release directory")
    resolved_entry = require_executable_entry(python_entry)

    fixed_venv_entry = venv_dir / "bin/python"
    if python_entry == fixed_venv_entry:
        require_unaliased_directory(venv_dir, "Hermes venv")
        bin_dir = fixed_venv_entry.parent
        if not bin_dir.is_dir() or bin_dir.is_symlink() or bin_dir.resolve() != bin_dir:
            raise ValueError("Hermes venv bin directory must not be aliased")
        pyvenv_config = venv_dir / "pyvenv.cfg"
        if not pyvenv_config.is_file() or pyvenv_config.is_symlink():
            raise ValueError("Hermes venv pyvenv.cfg must be a regular file")
        validate_venv_entry_target(resolved_entry, venv_dir, pyvenv_config)
        return "venv"

    try:
        python_entry.relative_to(release_dir)
        resolved_entry.relative_to(release_dir)
    except ValueError as exc:
        raise ValueError(
            "Hermes Python must use the fixed venv entry or remain inside the release"
        ) from exc
    return "release"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", required=True)
    parser.add_argument("--venv-dir", required=True)
    parser.add_argument("--release-dir", required=True)
    args = parser.parse_args()
    try:
        scope = validate(
            absolute_path(args.python, "Hermes Python"),
            absolute_path(args.venv_dir, "Hermes venv"),
            absolute_path(args.release_dir, "release directory"),
        )
    except (OSError, UnicodeError, ValueError) as exc:
        print(f"Hermes Python validation failed: {exc}", file=os.sys.stderr)
        return 1
    print(f"Hermes Python entry validated: {scope}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
