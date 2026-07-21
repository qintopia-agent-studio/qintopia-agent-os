#!/usr/bin/env python3
"""Affirm that the installed Hermes runtime resolves Erhua's named provider."""

from __future__ import annotations

import argparse
import os
from pathlib import Path

import yaml


EXPECTED_PROVIDER = "custom:livecool.net"
EXPECTED_BASE_URL = "https://livecool.net/v1"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True)
    args = parser.parse_args()
    try:
        from hermes_cli.config import get_compatible_custom_providers
        from hermes_cli.providers import resolve_provider_full

        config = yaml.safe_load(Path(args.config).read_text(encoding="utf-8")) or {}
        model = config.get("model") if isinstance(config, dict) else None
        if not isinstance(model, dict) or model.get("provider") != EXPECTED_PROVIDER:
            raise ValueError("Erhua model provider does not match the approved provider")
        user_providers = config.get("providers")
        if user_providers is not None and not isinstance(user_providers, dict):
            raise ValueError("Hermes providers config must be a mapping")
        custom_providers = get_compatible_custom_providers(config)
        resolved = resolve_provider_full(
            EXPECTED_PROVIDER,
            user_providers,
            custom_providers,
        )
        if (
            resolved is None
            or resolved.id != EXPECTED_PROVIDER
            or resolved.base_url.rstrip("/") != EXPECTED_BASE_URL.rstrip("/")
            or resolved.source != "user-config"
        ):
            raise ValueError("Hermes did not resolve the approved Livecool provider")
    except (ImportError, OSError, TypeError, ValueError, yaml.YAMLError) as exc:
        print(f"Hermes runtime provider verification failed: {exc}", file=os.sys.stderr)
        return 1
    print("Hermes runtime provider resolved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
