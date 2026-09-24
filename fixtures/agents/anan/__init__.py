"""Independent local Hermes registration for Anan's bounded task tools."""
from __future__ import annotations
import importlib.util
from pathlib import Path


def register(ctx):
    path = Path(__file__).resolve().parents[3] / "skills" / "person-foundation" / "__init__.py"
    spec = importlib.util.spec_from_file_location("qintopia_anan_foundation", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("person-foundation package unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.register(ctx, agent_id="anan")
