import importlib.machinery
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "bin" / "qintopia-collab-mcp"


def load_module():
    loader = importlib.machinery.SourceFileLoader("qintopia_collab_mcp_test", str(SCRIPT))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class CollabMcpPosterMigrationTests(unittest.TestCase):
    def call_agent(self, capability: str):
        module = load_module()
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "collab.db"
            old_database = os.environ.get("QINTOPIA_COLLAB_DB")
            os.environ["QINTOPIA_COLLAB_DB"] = str(database)
            try:
                result = module.call_tool(
                    "qintopia_collab_call_agent",
                    {
                        "caller": "xiaoman",
                        "source_agent": "xiaoman",
                        "target_agent": "huabaosi",
                        "capability": capability,
                        "request": "生成海报",
                    },
                )
            finally:
                if old_database is None:
                    os.environ.pop("QINTOPIA_COLLAB_DB", None)
                else:
                    os.environ["QINTOPIA_COLLAB_DB"] = old_database
        return result

    def test_poster_production_returns_migration_without_starting_hermes(self):
        result = self.call_agent("poster_production_request")
        self.assertFalse(result["success"])
        self.assertEqual(result["error"], "poster_production_moved_to_agentos_intake")
        self.assertIn("trusted Xiaoman Feishu conversation", result["message"])
        self.assertEqual(result["recommended_tool"], "qintopia_xiaoman_poster_production_request")
        self.assertFalse(result["retryable"])
        self.assertFalse(result["external_send_executed"])
        self.assertFalse(result["agent_subprocess_executed"])

    def test_non_poster_direct_agent_call_returns_control_plane_migration(self):
        result = self.call_agent("lightweight_design_proposal")

        self.assertFalse(result["success"])
        self.assertEqual(result["error"], "direct_agent_call_moved_to_agentos_control_plane")
        self.assertIn("operations control-plane work items", result["message"])
        self.assertEqual(result["recommended_tool"], "qintopia_operations_work_item_create")
        self.assertFalse(result["retryable"])
        self.assertFalse(result["external_send_executed"])
        self.assertFalse(result["agent_subprocess_executed"])


if __name__ == "__main__":
    unittest.main()
