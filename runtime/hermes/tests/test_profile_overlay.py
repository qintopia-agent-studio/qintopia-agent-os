from __future__ import annotations

import hashlib
import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[3]
RUNTIME = ROOT / "runtime/hermes"
RENDERER = RUNTIME / "render_profile_overlay.py"
MIGRATOR = RUNTIME / "migrate_erhua_livecool_env.py"
TRANSACTION = RUNTIME / "profile_transaction.py"
VERIFIER = RUNTIME / "verify_runtime_provider.py"
PYTHON_VALIDATOR = RUNTIME / "validate_hermes_python.py"
ACTIVATOR = ROOT / "deploy/runner/activate-erhua-profile.sh"
ROLLBACK = ROOT / "deploy/runner/rollback-erhua-profile.sh"
SMOKE = ROOT / "deploy/runner/smoke-release.sh"
OVERLAY = ROOT / "agents/erhua/config.template.yaml"
FIXTURES = Path(__file__).parent / "fixtures"
RELEASE_SHA = "0123456789abcdef0123456789abcdef01234567"


class ProfileOverlayTests(unittest.TestCase):
    def run_tool(self, tool: Path, *args: str, expect: int = 0) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [os.sys.executable, str(tool), *args],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(expect, result.returncode, result.stderr)
        return result

    def render(self, directory: Path, base: Path | None = None) -> tuple[Path, Path]:
        output = directory / "config.yaml"
        report = directory / "report.json"
        self.run_tool(
            RENDERER,
            "render",
            "--base",
            str(base or FIXTURES / "erhua-base.yaml"),
            "--overlay",
            str(OVERLAY),
            "--output",
            str(output),
            "--report",
            str(report),
        )
        return output, report

    def test_render_is_field_limited_idempotent_redacted_and_0600(self) -> None:
        with tempfile.TemporaryDirectory() as first_dir, tempfile.TemporaryDirectory() as second_dir:
            first, report_path = self.render(Path(first_dir))
            second, second_report = self.render(Path(second_dir), first)
            original = yaml.safe_load((FIXTURES / "erhua-base.yaml").read_text())
            rendered = yaml.safe_load(first.read_text())
            repeated = yaml.safe_load(second.read_text())

            self.assertEqual(rendered, repeated)
            self.assertEqual(original["channel"], rendered["channel"])
            self.assertEqual(original["unrelated_flag"], rendered["unrelated_flag"])
            self.assertEqual(0.3, rendered["model"]["temperature"])
            self.assertEqual(original["custom_providers"][0], rendered["custom_providers"][0])
            self.assertEqual("custom:livecool.net", rendered["model"]["provider"])
            livecool = rendered["custom_providers"][1]
            self.assertEqual("LIVECOOL_API_KEY", livecool["key_env"])
            self.assertNotIn("api_key", livecool)
            self.assertEqual(0o600, stat.S_IMODE(first.stat().st_mode))
            self.assertEqual(0o600, stat.S_IMODE(report_path.stat().st_mode))
            report_text = report_path.read_text()
            self.assertNotIn("fixture-livecool-key-not-real", report_text)
            self.assertTrue(json.loads(report_text)["secret_values_redacted"])
            self.assertEqual("unchanged", json.loads(second_report.read_text())["status"])

    def test_existing_inline_provider_is_replaced_without_secret(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory) / "base.yaml"
            base_config = yaml.safe_load((FIXTURES / "erhua-base.yaml").read_text())
            base_config["custom_providers"].append(
                {
                    "name": "Livecool.net",
                    "base_url": "https://old.example/v1",
                    "model": "old",
                    "api_key": "must-not-survive",
                    "timeout": 45,
                }
            )
            base.write_text(yaml.safe_dump(base_config, sort_keys=False))
            output, report = self.render(Path(directory), base)
            self.assertNotIn("must-not-survive", output.read_text())
            self.assertNotIn("must-not-survive", report.read_text())
            self.assertEqual(45, yaml.safe_load(output.read_text())["custom_providers"][1]["timeout"])

    def test_runtime_provider_verifier_requires_affirmative_hermes_resolution(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            config, _ = self.render(directory)
            package = directory / "hermes_cli"
            package.mkdir()
            (package / "__init__.py").write_text("")
            (package / "config.py").write_text(
                "def get_compatible_custom_providers(config):\n"
                "    return config.get('custom_providers', [])\n"
            )
            (package / "providers.py").write_text(
                "from types import SimpleNamespace\n"
                "def resolve_provider_full(name, user_providers, custom_providers):\n"
                "    if any(item.get('name') == 'Livecool.net' for item in custom_providers):\n"
                "        return SimpleNamespace(id=name, base_url='https://livecool.net/v1', source='user-config')\n"
                "    return None\n"
            )
            environment = {**os.environ, "PYTHONPATH": str(directory)}
            resolved = subprocess.run(
                [os.sys.executable, str(VERIFIER), "--config", str(config)],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(0, resolved.returncode, resolved.stderr)
            self.assertIn("runtime provider resolved", resolved.stdout)

            broken = yaml.safe_load(config.read_text())
            broken["custom_providers"] = []
            config.write_text(yaml.safe_dump(broken, sort_keys=False))
            rejected = subprocess.run(
                [os.sys.executable, str(VERIFIER), "--config", str(config)],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(0, rejected.returncode)

    def test_hermes_python_validator_accepts_venv_base_and_contains_release_entries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            venv = directory / "venv"
            venv_bin = venv / "bin"
            release = directory / "release"
            external = directory / "external-python"
            venv_bin.mkdir(parents=True)
            release.mkdir()
            (venv / "pyvenv.cfg").write_text("home = /external\n")
            external.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(external, 0o755)
            (venv_bin / "python").symlink_to(external)

            self.run_tool(
                PYTHON_VALIDATOR,
                "--python",
                str(venv_bin / "python"),
                "--venv-dir",
                str(venv),
                "--release-dir",
                str(release),
            )

            rejected_external = self.run_tool(
                PYTHON_VALIDATOR,
                "--python",
                str(external),
                "--venv-dir",
                str(venv),
                "--release-dir",
                str(release),
                expect=1,
            )
            self.assertIn("fixed venv entry or remain inside", rejected_external.stderr)

            release_python = release / "python"
            release_python.symlink_to(external)
            escaped_release = self.run_tool(
                PYTHON_VALIDATOR,
                "--python",
                str(release_python),
                "--venv-dir",
                str(venv),
                "--release-dir",
                str(release),
                expect=1,
            )
            self.assertIn("fixed venv entry or remain inside", escaped_release.stderr)
            release_python.unlink()
            release_python.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(release_python, 0o755)
            self.run_tool(
                PYTHON_VALIDATOR,
                "--python",
                str(release_python),
                "--venv-dir",
                str(venv),
                "--release-dir",
                str(release),
            )

            (venv / "pyvenv.cfg").unlink()
            missing_metadata = self.run_tool(
                PYTHON_VALIDATOR,
                "--python",
                str(venv_bin / "python"),
                "--venv-dir",
                str(venv),
                "--release-dir",
                str(release),
                expect=1,
            )
            self.assertIn("pyvenv.cfg", missing_metadata.stderr)

    def test_duplicate_alias_and_forbidden_overlay_fail_without_output(self) -> None:
        invalid_documents = {
            "duplicate.yaml": "model: {default: x}\nmodel: {default: y}\ncustom_providers: []\n",
            "alias.yaml": "model: &model {default: x}\nother: *model\ncustom_providers: []\n",
        }
        for name, document in invalid_documents.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                base = Path(directory) / name
                base.write_text(document)
                output = Path(directory) / "output.yaml"
                report = Path(directory) / "report.json"
                self.run_tool(
                    RENDERER,
                    "render",
                    "--base",
                    str(base),
                    "--overlay",
                    str(OVERLAY),
                    "--output",
                    str(output),
                    "--report",
                    str(report),
                    expect=1,
                )
                self.assertFalse(output.exists())
                self.assertFalse(report.exists())

        with tempfile.TemporaryDirectory() as directory:
            bad_overlay = Path(directory) / "overlay.yaml"
            bad_overlay.write_text(OVERLAY.read_text() + "forbidden: true\n")
            output = Path(directory) / "output.yaml"
            self.run_tool(
                RENDERER,
                "render",
                "--base",
                str(FIXTURES / "erhua-base.yaml"),
                "--overlay",
                str(bad_overlay),
                "--output",
                str(output),
                "--report",
                str(Path(directory) / "report.json"),
                expect=1,
            )
            self.assertFalse(output.exists())

    def test_symlinked_input_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            alias = Path(directory) / "base.yaml"
            alias.symlink_to(FIXTURES / "erhua-base.yaml")
            output = Path(directory) / "output.yaml"
            self.run_tool(
                RENDERER,
                "render",
                "--base",
                str(alias),
                "--overlay",
                str(OVERLAY),
                "--output",
                str(output),
                "--report",
                str(Path(directory) / "report.json"),
                expect=1,
            )
            self.assertFalse(output.exists())

    def test_env_migration_preserves_values_and_never_reports_secret(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            env = directory / "erhua.env"
            env.write_text("OTHER=value\n")
            output = directory / "candidate.env"
            report = directory / "report.json"
            self.run_tool(
                MIGRATOR,
                "prepare",
                "--env",
                str(env),
                "--default-config",
                str(FIXTURES / "default-with-livecool.yaml"),
                "--output",
                str(output),
                "--report",
                str(report),
            )
            self.assertIn("OTHER=value", output.read_text())
            self.assertIn("LIVECOOL_API_KEY=", output.read_text())
            self.assertNotIn("fixture-livecool-key-not-real", report.read_text())
            self.assertEqual(0o600, stat.S_IMODE(output.stat().st_mode))
            self.assertEqual("migrated", json.loads(report.read_text())["status"])
            self.run_tool(MIGRATOR, "check", "--env", str(output))
            repeated = directory / "repeated.env"
            repeated_report = directory / "repeated-report.json"
            self.run_tool(
                MIGRATOR,
                "prepare",
                "--env",
                str(output),
                "--default-config",
                str(FIXTURES / "default-with-livecool.yaml"),
                "--output",
                str(repeated),
                "--report",
                str(repeated_report),
            )
            self.assertEqual(output.read_bytes(), repeated.read_bytes())
            self.assertEqual("existing", json.loads(repeated_report.read_text())["status"])

    def test_env_conflict_fails_before_writing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            env = directory / "erhua.env"
            env.write_text("LIVECOOL_API_KEY=one\nLIVECOOL_API_KEY=two\n")
            output = directory / "candidate.env"
            self.run_tool(
                MIGRATOR,
                "prepare",
                "--env",
                str(env),
                "--default-config",
                str(FIXTURES / "default-with-livecool.yaml"),
                "--output",
                str(output),
                "--report",
                str(directory / "report.json"),
                expect=1,
            )
            self.assertFalse(output.exists())

        for binding in ('LIVECOOL_API_KEY="   "\n', 'LIVECOOL_API_KEY="different"\n'):
            with self.subTest(binding=binding), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory).resolve()
                env = directory / "erhua.env"
                env.write_text(binding)
                output = directory / "candidate.env"
                self.run_tool(
                    MIGRATOR,
                    "prepare",
                    "--env",
                    str(env),
                    "--default-config",
                    str(FIXTURES / "default-with-livecool.yaml"),
                    "--output",
                    str(output),
                    "--report",
                    str(directory / "report.json"),
                    expect=1,
                )
                self.assertFalse(output.exists())

    def test_runner_dry_run_activation_and_exact_rollback(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            profile = directory / "profile"
            state = directory / "state"
            profile.mkdir()
            state.mkdir()
            config = profile / "config.yaml"
            env_file = profile / ".env"
            default_config = directory / "default.yaml"
            fake_hermes_python = ROOT / "runtime/hermes/tests/.test-hermes-python"
            config.write_text((FIXTURES / "erhua-base.yaml").read_text())
            env_file.write_text("OTHER=value\n")
            default_config.write_text((FIXTURES / "default-with-livecool.yaml").read_text())
            fake_hermes_python.write_text(
                '#!/bin/sh\ntouch "${0}.called"\nexit 0\n'
            )
            os.chmod(fake_hermes_python, 0o755)
            os.chmod(config, 0o640)
            os.chmod(env_file, 0o640)
            before_config = config.read_bytes()
            before_env = env_file.read_bytes()
            before_config_owner = (config.stat().st_uid, config.stat().st_gid)
            before_env_owner = (env_file.stat().st_uid, env_file.stat().st_gid)
            dry_run_request_id = "deploy-20260721T010203Z-0123456789ab"
            request_id = "deploy-20260721T010205Z-0123456789ab"
            environment = {
                **os.environ,
                "QINTOPIA_ERHUA_PROFILE_DIR": str(profile),
                "QINTOPIA_DEFAULT_HERMES_CONFIG": str(default_config),
                "QINTOPIA_HERMES_PYTHON": str(fake_hermes_python),
            }

            dry_run = subprocess.run(
                [
                    "bash",
                    str(ACTIVATOR),
                    "--release-dir",
                    str(ROOT),
                    "--state-dir",
                    str(state),
                    "--request-id",
                    dry_run_request_id,
                    "--release-sha",
                    RELEASE_SHA,
                    "--dry-run",
                ],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(0, dry_run.returncode, dry_run.stderr)
            self.assertNotIn("fixture-livecool-key-not-real", dry_run.stdout + dry_run.stderr)
            self.assertEqual(before_config, config.read_bytes())
            self.assertEqual(before_env, env_file.read_bytes())
            self.assertFalse((state / "profile-backups").exists())
            self.assertTrue(Path(f"{fake_hermes_python}.called").exists())

            alias = directory / "profile-alias"
            alias.symlink_to(profile, target_is_directory=True)
            aliased = subprocess.run(
                [
                    "bash",
                    str(ACTIVATOR),
                    "--release-dir",
                    str(ROOT),
                    "--state-dir",
                    str(state),
                    "--request-id",
                    "deploy-20260721T010204Z-0123456789ab",
                    "--release-sha",
                    RELEASE_SHA,
                    "--dry-run",
                ],
                cwd=ROOT,
                env={
                    **environment,
                    "QINTOPIA_ERHUA_PROFILE_DIR": str(alias),
                },
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(0, aliased.returncode)
            self.assertIn("must not contain path aliases", aliased.stderr)
            marker = (
                state
                / "profile-dry-runs"
                / f"erhua-{RELEASE_SHA}-{dry_run_request_id}.json"
            )
            self.assertEqual(0o600, stat.S_IMODE(marker.stat().st_mode))
            self.assertIn("before_sha256", json.loads(marker.read_text())["secret_binding"])
            public_evidence = json.loads(
                (state / "results" / f"{dry_run_request_id}.profile.json").read_text()
            )
            self.assertNotIn("before_sha256", public_evidence["secret_binding"])
            self.assertNotIn("after_sha256", public_evidence["secret_binding"])

            env_file.write_text("OTHER=changed-after-dry-run\n")
            drifted = subprocess.run(
                [
                    "bash",
                    str(ACTIVATOR),
                    "--release-dir",
                    str(ROOT),
                    "--state-dir",
                    str(state),
                    "--request-id",
                    request_id,
                    "--release-sha",
                    RELEASE_SHA,
                    "--dry-run-request-id",
                    dry_run_request_id,
                ],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(0, drifted.returncode)
            self.assertIn("runtime state changed", drifted.stderr)
            self.assertFalse((state / "profile-backups").exists())
            env_file.write_bytes(before_env)

            activated = subprocess.run(
                [
                    "bash",
                    str(ACTIVATOR),
                    "--release-dir",
                    str(ROOT),
                    "--state-dir",
                    str(state),
                    "--request-id",
                    request_id,
                    "--release-sha",
                    RELEASE_SHA,
                    "--dry-run-request-id",
                    dry_run_request_id,
                ],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(0, activated.returncode, activated.stderr)
            self.assertNotEqual(before_config, config.read_bytes())
            self.assertEqual(0o600, stat.S_IMODE(config.stat().st_mode))
            self.assertEqual(
                before_config_owner, (config.stat().st_uid, config.stat().st_gid)
            )
            self.assertEqual(before_env_owner, (env_file.stat().st_uid, env_file.stat().st_gid))
            backup = state / "profile-backups" / request_id
            self.assertEqual(0o600, stat.S_IMODE((backup / "config.yaml").stat().st_mode))
            self.assertEqual(0o600, stat.S_IMODE((backup / "erhua.env").stat().st_mode))
            self.assertNotIn("fixture-livecool-key-not-real", (backup / "metadata.json").read_text())
            activated_evidence = json.loads(
                (state / "results" / f"{request_id}.profile.json").read_text()
            )
            self.assertNotIn(
                "sha256", activated_evidence["file_transaction"]["files"]["env"]
            )
            self.run_tool(
                TRANSACTION,
                "verify-activated",
                "--config",
                str(config),
                "--env",
                str(env_file),
                "--backup-dir",
                str(backup),
                "--metadata",
                str(backup / "metadata.json"),
            )

            rolled_back = subprocess.run(
                [
                    "bash",
                    str(ROLLBACK),
                    "--release-dir",
                    str(ROOT),
                    "--state-dir",
                    str(state),
                    "--request-id",
                    request_id,
                    "--evidence-output",
                    str(state / "results" / f"{request_id}.restore.json"),
                ],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(0, rolled_back.returncode, rolled_back.stderr)
            self.assertEqual(before_config, config.read_bytes())
            self.assertEqual(before_env, env_file.read_bytes())
            self.assertEqual(0o640, stat.S_IMODE(config.stat().st_mode))
            self.assertEqual(0o640, stat.S_IMODE(env_file.stat().st_mode))
            self.assertEqual(
                before_config_owner, (config.stat().st_uid, config.stat().st_gid)
            )
            self.assertEqual(before_env_owner, (env_file.stat().st_uid, env_file.stat().st_gid))
            restore_evidence = json.loads(
                (state / "results" / f"{request_id}.restore.json").read_text()
            )
            self.assertEqual("restored", restore_evidence["phase"])
            self.assertNotIn("sha256", restore_evidence["files"]["env"])
            fake_hermes_python.unlink(missing_ok=True)
            called = Path(f"{fake_hermes_python}.called")
            called.unlink(missing_ok=True)

    def test_activation_requires_matching_dry_run_before_backup_or_write(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            profile = directory / "profile"
            state = directory / "state"
            profile.mkdir()
            state.mkdir()
            config = profile / "config.yaml"
            env_file = profile / ".env"
            default_config = directory / "default.yaml"
            fake_hermes_python = ROOT / "runtime/hermes/tests/.test-hermes-python"
            config.write_bytes((FIXTURES / "erhua-base.yaml").read_bytes())
            env_file.write_text("OTHER=value\n")
            default_config.write_bytes(
                (FIXTURES / "default-with-livecool.yaml").read_bytes()
            )
            fake_hermes_python.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(fake_hermes_python, 0o755)
            before_config = config.read_bytes()
            before_env = env_file.read_bytes()
            result = subprocess.run(
                [
                    "bash",
                    str(ACTIVATOR),
                    "--release-dir",
                    str(ROOT),
                    "--state-dir",
                    str(state),
                    "--request-id",
                    "deploy-20260721T010203Z-0123456789ab",
                    "--release-sha",
                    RELEASE_SHA,
                    "--dry-run-request-id",
                    "deploy-20260721T010200Z-0123456789ab",
                ],
                cwd=ROOT,
                env={
                    **os.environ,
                    "QINTOPIA_ERHUA_PROFILE_DIR": str(profile),
                    "QINTOPIA_DEFAULT_HERMES_CONFIG": str(default_config),
                    "QINTOPIA_HERMES_PYTHON": str(fake_hermes_python),
                },
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(0, result.returncode)
            self.assertIn("dry-run evidence is required", result.stderr)
            self.assertEqual(before_config, config.read_bytes())
            self.assertEqual(before_env, env_file.read_bytes())
            self.assertFalse((state / "profile-backups").exists())
            fake_hermes_python.unlink(missing_ok=True)

    def test_transaction_restores_files_when_activation_metadata_write_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            profile = directory / "profile"
            backup = directory / "backup"
            profile.mkdir()
            config = profile / "config.yaml"
            env_file = profile / ".env"
            candidate_config = directory / "candidate-config.yaml"
            candidate_env = directory / "candidate.env"
            metadata = backup / "metadata.json"
            config.write_text("model: {default: before}\n")
            env_file.write_text("BEFORE=value\n")
            candidate_config.write_text("model: {default: after}\n")
            candidate_env.write_text("AFTER=value\n")
            os.chmod(config, 0o640)
            os.chmod(env_file, 0o640)

            common = (
                "--config",
                str(config),
                "--env",
                str(env_file),
                "--backup-dir",
                str(backup),
                "--metadata",
                str(metadata),
            )
            self.run_tool(
                TRANSACTION,
                "backup",
                *common,
                "--expected-config-sha",
                hashlib.sha256(config.read_bytes()).hexdigest(),
                "--expected-env-sha",
                hashlib.sha256(env_file.read_bytes()).hexdigest(),
            )
            os.chmod(backup, 0o500)
            try:
                self.run_tool(
                    TRANSACTION,
                    "activate",
                    *common,
                    "--candidate-config",
                    str(candidate_config),
                    "--candidate-env",
                    str(candidate_env),
                    expect=1,
                )
            finally:
                os.chmod(backup, 0o700)

            self.assertEqual("model: {default: before}\n", config.read_text())
            self.assertEqual("BEFORE=value\n", env_file.read_text())
            self.assertEqual(0o640, stat.S_IMODE(config.stat().st_mode))
            self.assertEqual(0o640, stat.S_IMODE(env_file.stat().st_mode))

    def test_transaction_rejects_drift_and_removes_incomplete_backup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            config = directory / "config.yaml"
            env_file = directory / ".env"
            backup = directory / "backup"
            config.write_text("model: {default: current}\n")
            env_file.write_text("KEY=current\n")
            result = self.run_tool(
                TRANSACTION,
                "backup",
                "--config",
                str(config),
                "--env",
                str(env_file),
                "--backup-dir",
                str(backup),
                "--metadata",
                str(backup / "metadata.json"),
                "--expected-config-sha",
                "0" * 64,
                "--expected-env-sha",
                hashlib.sha256(env_file.read_bytes()).hexdigest(),
                expect=1,
            )
            self.assertIn("changed after the reviewed dry run", result.stderr)
            self.assertFalse(backup.exists())

    def test_smoke_rejects_unknown_provider_even_when_doctor_exits_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            release_root = directory / "releases"
            release = release_root / "current"
            profile = directory / "profile"
            fake_bin = directory / "bin"
            (release / "runtime/hermes").mkdir(parents=True)
            (release / "agents/erhua").mkdir(parents=True)
            profile.mkdir()
            fake_bin.mkdir()
            for source in (RENDERER, MIGRATOR, VERIFIER):
                (release / "runtime/hermes" / source.name).write_bytes(source.read_bytes())
            (release / "agents/erhua/config.template.yaml").write_bytes(OVERLAY.read_bytes())
            self.render(profile)
            env_file = profile / ".env"
            env_file.write_text('LIVECOOL_API_KEY="not-a-real-secret"\n')
            fake_runuser = fake_bin / "runuser"
            fake_runuser.write_text(
                "#!/bin/sh\n"
                "case \"$*\" in\n"
                "  *\" doctor\") echo \"model.provider 'custom:livecool.net' is not a recognized provider\" ;;\n"
                "esac\n"
                "exit 0\n"
            )
            os.chmod(fake_runuser, 0o755)
            result = subprocess.run(
                [
                    "bash",
                    str(SMOKE),
                    "--release-root",
                    str(release_root),
                    "--restart-targets",
                    "hermes-erhua",
                ],
                cwd=ROOT,
                env={
                    **os.environ,
                    "PATH": f"{fake_bin}:{os.environ['PATH']}",
                    "QINTOPIA_ERHUA_PROFILE_DIR": str(profile),
                    "QINTOPIA_HERMES_BIN": "/fake/hermes",
                    "QINTOPIA_HERMES_PYTHON": os.sys.executable,
                },
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(0, result.returncode)
            self.assertIn("did not recognise the Livecool provider", result.stderr)

            fake_runuser.write_text("#!/bin/sh\nexit 0\n")
            evidence = directory / "smoke.json"
            succeeded = subprocess.run(
                [
                    "bash",
                    str(SMOKE),
                    "--release-root",
                    str(release_root),
                    "--restart-targets",
                    "hermes-erhua",
                    "--evidence-output",
                    str(evidence),
                ],
                cwd=ROOT,
                env={
                    **os.environ,
                    "PATH": f"{fake_bin}:{os.environ['PATH']}",
                    "QINTOPIA_ERHUA_PROFILE_DIR": str(profile),
                    "QINTOPIA_HERMES_BIN": "/fake/hermes",
                    "QINTOPIA_HERMES_PYTHON": os.sys.executable,
                },
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(0, succeeded.returncode, succeeded.stderr)
            smoke_evidence = json.loads(evidence.read_text())
            self.assertTrue(smoke_evidence["service_active"])
            self.assertTrue(smoke_evidence["doctor_succeeded"])
            self.assertTrue(smoke_evidence["runtime_provider_resolved"])
            self.assertFalse(smoke_evidence["inference_called"])
            self.assertFalse(smoke_evidence["external_delivery"])
            self.assertEqual(0o600, stat.S_IMODE(evidence.stat().st_mode))


if __name__ == "__main__":
    unittest.main()
