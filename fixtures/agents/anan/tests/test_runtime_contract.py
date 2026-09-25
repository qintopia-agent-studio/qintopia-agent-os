from pathlib import Path
import importlib.util
import json
import os
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[4]


class AnanRuntimeContract(unittest.TestCase):
    def test_independent_plugin_does_not_register_other_agents_or_send_tools(self):
        spec = importlib.util.spec_from_file_location("anan_runtime_test", ROOT / "fixtures/agents/anan/__init__.py")
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

    def test_fixture_is_not_deployed_or_enrolled_in_core_upgrade(self):
        # The real Anan service is now managed. This simulated runtime must remain
        # excluded from its payload and launch chain; core enrollment is separate.
        for path in ("tools/deploy/build-deploy-bundle.mjs", "deploy/restart-target-rules.yaml",
                     "deploy/runner/smoke-release.sh"):
            text = (ROOT / path).read_text()
            self.assertNotIn('"fixtures/agents"', text)
            self.assertNotIn("fixtures/agents/anan", text)
        registry = (ROOT / "runtime/hermes/profile-registry.yaml").read_text()
        self.assertNotIn("hermes-anan", registry)
        self.assertNotIn("agents/anan/agent.yaml", registry)


if __name__ == "__main__":
    unittest.main()
