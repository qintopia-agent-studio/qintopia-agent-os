import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

RUNTIME = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RUNTIME))
import dashboard_artifact as artifact

SPEC = importlib.util.spec_from_file_location("dashboard_installer", RUNTIME.parents[1] / "deploy/runner/install-hermes-dashboard.py")
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


class DashboardArtifactTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.dist = self.root / "artifact"
        self.core = self.root / ("a" * 40)
        self.dist.mkdir()
        (self.dist / "web_dist/assets").mkdir(parents=True)
        (self.dist / "web_dist/index.html").write_text('<script type="module" src="/assets/main.js"></script>')
        (self.dist / "web_dist/assets/main.js").write_text("console.log('fixture')")
        (self.dist / "pnpm-lock.yaml").write_text("lockfileVersion: '9.0'\n")
        core_files = {}
        for rel in ["hermes_cli/web_server.py", "hermes_cli/main_dashboard.py", "web/package.json", "package-lock.json"]:
            p = self.core / "core" / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("fixture")
            core_files[rel] = artifact.digest(p)
        (self.core / "artifact-manifest.json").write_text(json.dumps({"commit_sha": self.core.name, "repository": "https://github.com/NousResearch/hermes-agent.git"}))
        runtime_files = {}
        for name in ["dashboard_launcher.py", "dashboard_artifact.py"]:
            (self.dist / name).write_bytes((RUNTIME / name).read_bytes())
            runtime_files[name] = artifact.digest(self.dist / name)
        self.manifest = {
            "schema_version": 1, "core_commit": self.core.name, "core_files": core_files,
            "files": artifact.inventory(self.dist / "web_dist"),
            "pnpm_lock_sha256": artifact.digest(self.dist / "pnpm-lock.yaml"),
            "source_archive_sha256": "b" * 64, "build_command": ["fixture"],
            "upstream_typecheck": "passed", "runtime_files": runtime_files,
        }
        self.write_manifest()

    def write_manifest(self):
        (self.dist / "dashboard-manifest.json").write_text(json.dumps(self.manifest))
        self.checksum = artifact.digest(self.dist / "dashboard-manifest.json")

    def verify(self):
        return artifact.validate(self.dist, self.core, self.checksum)

    def test_valid_artifact_and_pinned_unit(self):
        self.verify()
        unit = installer.render_unit(installer.ROOT / "releases" / self.checksum, self.core.name, self.checksum)
        self.assertIn(self.core.name + "/runtime/venv/bin/python -I", unit)
        self.assertNotIn(".local/bin/hermes", unit)
        self.assertNotIn("gateway", unit)
        self.assertIn("--check", unit)

    def test_missing_asset_rejected_even_with_updated_manifest(self):
        (self.dist / "web_dist/assets/main.js").unlink()
        self.manifest["files"] = artifact.inventory(self.dist / "web_dist")
        self.write_manifest()
        with self.assertRaisesRegex(artifact.ArtifactError, "entry_asset_missing"):
            self.verify()

    def test_tampering_backend_and_frontend_rejected(self):
        (self.dist / "web_dist/assets/main.js").write_text("tampered")
        with self.assertRaisesRegex(artifact.ArtifactError, "dist_inventory_mismatch"):
            self.verify()
        (self.dist / "web_dist/assets/main.js").write_text("console.log('fixture')")
        (self.core / "core/hermes_cli/web_server.py").write_text("new backend")
        with self.assertRaisesRegex(artifact.ArtifactError, "core_file_mismatch"):
            self.verify()

    def test_payload_validation_needs_no_installed_core(self):
        self.assertEqual(artifact.validate_payload(self.dist, self.checksum), self.manifest)

    def test_manifest_digest_required(self):
        with self.assertRaisesRegex(artifact.ArtifactError, "manifest_digest_mismatch"):
            artifact.validate(self.dist, self.core, "f" * 64)

    def test_symlink_and_unlisted_root_rejected(self):
        (self.dist / "extra").symlink_to(self.root)
        with self.assertRaisesRegex(artifact.ArtifactError, "artifact_layout_invalid"):
            self.verify()
        (self.dist / "extra").unlink()
        (self.dist / "web_dist/assets/main.js").unlink()
        (self.dist / "web_dist/assets/main.js").symlink_to(self.dist / "pnpm-lock.yaml")
        with self.assertRaisesRegex(artifact.ArtifactError, "symlink_rejected"):
            self.verify()

    def test_wrong_core_and_runtime_rejected(self):
        self.manifest["core_commit"] = "c" * 40
        self.write_manifest()
        with self.assertRaisesRegex(artifact.ArtifactError, "core_commit_mismatch"):
            self.verify()
        self.manifest["core_commit"] = self.core.name
        self.write_manifest()
        (self.dist / "dashboard_launcher.py").write_text("tampered")
        with self.assertRaisesRegex(artifact.ArtifactError, "runtime_digest_mismatch"):
            self.verify()

    def test_external_and_traversal_entry_assets_rejected(self):
        for value in ["https://example.invalid/app.js", "/assets/%2e%2e/private"]:
            with self.subTest(value=value):
                (self.dist / "web_dist/index.html").write_text(f'<script src="{value}"></script>')
                self.manifest["files"] = artifact.inventory(self.dist / "web_dist")
                self.write_manifest()
                with self.assertRaises(artifact.ArtifactError):
                    self.verify()


if __name__ == "__main__":
    unittest.main()
