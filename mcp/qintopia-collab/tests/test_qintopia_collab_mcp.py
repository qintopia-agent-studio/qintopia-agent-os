import importlib.machinery
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).parents[1] / "bin" / "qintopia-collab-mcp"


def load_module():
    loader = importlib.machinery.SourceFileLoader("qintopia_collab_mcp_test", str(SCRIPT))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class CollabMcpPosterMigrationTests(unittest.TestCase):
    def test_poster_production_returns_migration_without_starting_hermes(self):
        module = load_module()
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "collab.db"
            with mock.patch.dict(os.environ, {"QINTOPIA_COLLAB_DB": str(database)}), mock.patch.object(
                module.subprocess, "run"
            ) as run:
                result = module.call_tool(
                    "qintopia_collab_call_agent",
                    {
                        "caller": "xiaoman",
                        "source_agent": "xiaoman",
                        "target_agent": "huabaosi",
                        "capability": "poster_production_request",
                        "request": "生成海报",
                    },
                )

        self.assertFalse(result["success"])
        self.assertEqual(result["error"], "poster_production_moved_to_agentos_intake")
        self.assertIn("trusted Xiaoman Feishu conversation", result["message"])
        self.assertFalse(result["retryable"])
        self.assertFalse(result["external_send_executed"])
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
