from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "pms_startup_preflight", Path(__file__).resolve().parents[1] / "startup_preflight.py")
preflight = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preflight)
SOURCE = Path(__file__).resolve().parents[1]
production_spec = importlib.util.spec_from_file_location("pms_preflight_test_production", SOURCE / "production.py")
production = importlib.util.module_from_spec(production_spec)
production_spec.loader.exec_module(production)


def check_static_with_json_fixtures(**kwargs):
    # The isolated business-test venv lacks the PyYAML supplied by Hermes at runtime.
    with patch.dict(sys.modules, {"yaml": SimpleNamespace(safe_load=json.loads)}):
        preflight.check_static(**kwargs)


class StartupPreflightTests(unittest.TestCase):
    def test_fixed_source_digest_and_readonly_inputs(self):
        with tempfile.TemporaryDirectory() as root:
            release = Path(root) / "release"
            installed = Path(root) / "installed"
            release.mkdir()
            installed.mkdir()
            for name in preflight.RUNTIME_FILES:
                shutil.copy2(SOURCE / name, release / name)
                shutil.copy2(SOURCE / name, installed / name)
            profile = Path(root) / "config.yaml"
            profile.write_text(json.dumps({"plugins": {}}))
            managed = Path(root) / "managed"
            managed.mkdir()
            (managed / "config.yaml").write_text(json.dumps({"hooks": {}}))
            digest = preflight.fingerprint(release, required_uid=os.getuid())
            readonly_calls = []
            with patch.object(preflight, "readonly", side_effect=lambda p, **_: readonly_calls.append(Path(p))):
                check_static_with_json_fixtures(installed=installed, release=release,
                    expected_sha256=digest, profile_config=profile,
                    managed_directory=managed, managed_check=lambda _: None,
                    required_uid=os.getuid())
            self.assertEqual(readonly_calls, [installed, profile])
            (installed / "production.py").write_text("# simulated replacement\n")
            with patch.object(preflight, "readonly"):
                with self.assertRaisesRegex(ValueError, preflight.ERROR):
                    check_static_with_json_fixtures(installed=installed, release=release,
                        expected_sha256=digest, profile_config=profile,
                        managed_directory=managed, managed_check=lambda _: None,
                        required_uid=os.getuid())
            (installed / "production.py").unlink()
            shutil.copy2(SOURCE / "production.py", installed / "production.py")
            profile.write_text(json.dumps({"hooks": {
                "pre_tool_call": [{"command": "/bin/true", "enabled": False}],
            }}))
            with patch.object(preflight, "readonly"):
                with self.assertRaises(ValueError):
                    check_static_with_json_fixtures(installed=installed, release=release,
                        expected_sha256=digest, profile_config=profile,
                        managed_directory=managed, managed_check=lambda _: None,
                        required_uid=os.getuid())
            profile.write_text(json.dumps({"hooks": {}}))
            (managed / "config.yaml").write_text(json.dumps({"hooks": {
                "post_tool_call": [{"command": "/bin/true"}],
            }}))
            with patch.object(preflight, "readonly"):
                with self.assertRaises(ValueError):
                    check_static_with_json_fixtures(installed=installed, release=release,
                        expected_sha256=digest, profile_config=profile,
                        managed_directory=managed, managed_check=lambda _: None,
                        required_uid=os.getuid())
            (installed / "shadow.py").write_text("# unreviewed module\n")
            with self.assertRaisesRegex(ValueError, preflight.ERROR):
                preflight.fingerprint(installed, required_uid=os.getuid())

    def test_writable_or_linked_runtime_path_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            config = Path(root) / "config.yaml"
            config.write_text("{}")
            with self.assertRaisesRegex(ValueError, preflight.ERROR):
                preflight.readonly(config)
            link = Path(root) / "config-link.yaml"
            link.symlink_to(config)
            with self.assertRaisesRegex(ValueError, preflight.ERROR):
                preflight.readonly(link)

    def test_loaded_plugin_must_be_the_reviewed_registration(self):
        with tempfile.TemporaryDirectory() as root:
            installed = Path(root)
            def callback(path):
                return SimpleNamespace(__code__=SimpleNamespace(co_filename=str(installed / path)))
            loaded = SimpleNamespace(enabled=True, error=None, deferred=False,
                module=SimpleNamespace(__file__=str(installed / "__init__.py"), production=production),
                manifest=SimpleNamespace(path=str(installed)),
                tools_registered=list(preflight.EXPECTED_TOOLS),
                hooks_registered=["pre_gateway_dispatch", "pre_tool_call", "pre_tool_call"])
            hooks = {"pre_gateway_dispatch": [callback("__init__.py")],
                     "pre_tool_call": [callback("__init__.py"), callback("production.py")]}
            manager = SimpleNamespace(_plugins={"pms-operations": loaded}, _hooks=hooks,
                                      scope_key="simulated-scope")
            registry = SimpleNamespace(get_entry=lambda name, scope:
                SimpleNamespace(handler=callback("__init__.py")))
            preflight.check_loaded_plugin(manager, registry, installed)
            for changed in ({"error": "simulated import failure"},
                            {"hooks_registered": ["pre_gateway_dispatch", "pre_tool_call"]},
                            {"module": SimpleNamespace(__file__="/other/__init__.py")},
                            {"tools_registered": []}):
                candidate = SimpleNamespace(**{**vars(loaded), **changed})
                with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, preflight.ERROR):
                    preflight.check_loaded_plugin(
                        SimpleNamespace(_plugins={"pms-operations": candidate}, _hooks=hooks,
                                        scope_key="simulated-scope"), registry, installed)
            manager._hooks["pre_tool_call"].pop()
            with self.assertRaisesRegex(ValueError, preflight.ERROR):
                preflight.check_loaded_plugin(manager, registry, installed)
            manager._hooks["pre_tool_call"].append(callback("production.py"))
            with self.assertRaisesRegex(ValueError, preflight.ERROR):
                preflight.check_loaded_plugin(manager, SimpleNamespace(get_entry=lambda *_args, **_kwargs: None), installed)
            shell = SimpleNamespace(__module__="agent.shell_hooks", __name__="shell_hook[pre_tool_call]")
            manager._hooks["pre_tool_call"].append(shell)
            with self.assertRaisesRegex(ValueError, preflight.ERROR):
                preflight.check_loaded_plugin(manager, registry, installed)


if __name__ == "__main__":
    unittest.main()
