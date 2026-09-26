import importlib.util
import json
import os
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("pms_credentials_test_plugin", Path(__file__).parents[1] / "__init__.py")
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)
credentials = plugin.credentials


class CredentialTests(unittest.TestCase):
    def setUp(self):
        workspace = Path(__file__).resolve().parents[3] / ".local-workspace"
        workspace.mkdir(exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(prefix="pms-credentials-", dir=workspace)
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.home = self.root / "profile"
        self.home.mkdir(mode=0o700)
        self.path = self.root / "credentials.json"
        self.values = {key: "simulated-" + str(index) + "x" * 32 for index, key in enumerate(sorted(credentials.KEYS))}
        self.write(self.values)

    def write(self, value):
        self.path.write_text(json.dumps(value))
        self.path.chmod(0o600)

    def load(self, path=None, environ=None):
        return credentials.load(str(path or self.path), profile_home=str(self.home), environ={} if environ is None else environ)

    def assert_denied(self, **kwargs):
        with self.assertRaisesRegex(ValueError, "^private_credentials_required$"):
            self.load(**kwargs)

    def test_reads_private_file_without_exporting_or_caching_credentials(self):
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(self.load(), self.values)
            self.assertFalse(credentials.KEYS & os.environ.keys())
            rotated = {key: value + "new" for key, value in self.values.items()}
            self.write(rotated)
            self.assertEqual(self.load(), rotated)

    def test_rejects_environment_duplicates_even_empty(self):
        for key in credentials.KEYS:
            for value in ("", "simulated"):
                self.assert_denied(environ={key: value})

    def test_matches_existing_client_and_broker_token_length_contracts(self):
        for size in (16, 512):
            self.write(dict(self.values, GREENPMS_API_TOKEN="p" * size))
            self.assertEqual(len(self.load()["GREENPMS_API_TOKEN"]), size)
        for key in credentials.KEYS - {"GREENPMS_API_TOKEN"}:
            for size in (31, 257):
                self.write(dict(self.values, **{key: "f" * size}))
                self.assert_denied()

    def test_rejects_file_and_parent_permissions_and_wrong_owner(self):
        for mode in (0o640, 0o644, 0o666, 0o700):
            self.path.chmod(mode)
            self.assert_denied()
        self.path.chmod(0o600)
        self.root.chmod(0o770)
        self.assert_denied()
        self.root.chmod(0o700)
        with patch.object(credentials.os, "getuid", return_value=os.getuid() + 1):
            self.assert_denied()

    def test_rejects_links_profile_paths_and_special_files(self):
        link = self.root / "link"
        link.symlink_to(self.path)
        self.assert_denied(path=link)
        link.unlink()
        link.symlink_to(self.root, target_is_directory=True)
        self.assert_denied(path=link / self.path.name)
        link.unlink()
        os.link(self.path, link)
        self.assert_denied()
        link.unlink()
        moved = self.home / self.path.name
        self.path.rename(moved)
        self.assert_denied(path=moved)
        os.mkfifo(self.path)
        self.assert_denied()

    def test_rejects_invalid_bounded_content_without_reflecting_it(self):
        variants = [[], {}, dict(self.values, extra="secret"),
                    {key: "same" * 10 for key in credentials.KEYS}]
        for key in credentials.KEYS:
            for value in (None, "short", "x" * 5000, "x" * 40 + "\n", "x" * 40 + "\x7f"):
                variants.append(dict(self.values, **{key: value}))
        for variant in variants:
            self.write(variant)
            self.assert_denied()
        for raw in ('{"GREENPMS_API_TOKEN":"private","GREENPMS_API_TOKEN":"private"}', "x" * 17000):
            self.path.write_text(raw)
            self.assert_denied()

    def test_plugin_uses_file_without_environment_fallback_or_production_activation(self):
        hermes = types.ModuleType("hermes_constants")
        hermes.get_hermes_home = lambda: self.home
        with patch.dict("sys.modules", {"hermes_constants": hermes}), patch.dict(os.environ, {
            "QINTOPIA_PMS_CREDENTIALS_FILE": str(self.path),
        }, clear=True):
            self.assertEqual(plugin.credential_values(), self.values)
            self.assertFalse(plugin.enabled())
            self.path.unlink()
            os.environ["GREENPMS_API_TOKEN"] = "simulated-env-fallback"
            with self.assertRaisesRegex(ValueError, "^private_credentials_required$"):
                plugin.credential_values()
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "^pms_disabled$"):
                plugin.credential_values()


if __name__ == "__main__":
    unittest.main()
