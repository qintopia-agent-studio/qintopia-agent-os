from pathlib import Path
import importlib.util
import json
import os
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]


class AnanRuntimeContract(unittest.TestCase):
    def test_independent_plugin_does_not_register_other_agents_or_send_tools(self):
        spec = importlib.util.spec_from_file_location("anan_runtime_test", ROOT / "agents/anan/__init__.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        class Context:
            def __init__(self):
                self.tools = {}
            def register_tool(self, **tool):
                self.tools[tool["name"]] = tool
        ctx = Context()
        module.register(ctx)
        self.assertEqual(set(ctx.tools), {"qintopia_person_context", "qintopia_person_task_status"})
        with patch.dict(os.environ, {}, clear=True):
            result = json.loads(ctx.tools["qintopia_person_context"]["handler"]({}))
        self.assertEqual(result, {"ok": False, "error": {"code": "foundation_disabled"}})

    def test_registration_never_enables_a_production_runtime(self):
        for path in ("runtime/hermes/profile-registry.yaml", "deploy/restart-target-rules.yaml",
                     "deploy/runner/smoke-release.sh"):
            text = (ROOT / path).read_text()
            self.assertNotIn("hermes-anan", text)
            self.assertNotIn("agents/anan/agent.yaml", text)


if __name__ == "__main__":
    unittest.main()
