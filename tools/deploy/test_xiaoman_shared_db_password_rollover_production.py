#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import signal
import stat
import subprocess
import sys
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest import mock


sys.dont_write_bytecode = True

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = (
    REPO_ROOT
    / "deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py"
)
SPEC = importlib.util.spec_from_file_location("xiaoman_db_rollover", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load Xiaoman database rollover module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)

PRODUCTION_RELEASE_RESTART_TARGETS = [
    "qintopia-system-services",
    "hermes-erhua",
]


def sha256(value: str | bytes) -> str:
    data = value if isinstance(value, bytes) else value.encode("utf-8")
    return hashlib.sha256(data).hexdigest()


class FakeOperations:
    def __init__(self, approved, *, alter_result="success") -> None:
        self.approved = approved
        self.old_url = (
            "postgresql://shared_role:old-password@db-a:5432,db-b:5432/"
            "qintopia?sslmode=verify-full&target_session_attrs=read-write"
        )
        self.role_name = "shared_role"
        self.chat_id = "oc_private"
        self.user_id = "ou_requester"
        self.credential_mode = "unrotated"
        self.config_url = self.old_url
        self.config_state = "disabled"
        self.database_binding = "old"
        self.runtime_ready = True
        self.policy_applied = False
        self.policy_version = 1
        self.policy_digest = "sha256:" + "9" * 64
        self.alter_result = alter_result
        self.alter_calls = 0
        self.config_calls = []
        self.config_stage_cleanup_calls = 0
        self.policy_calls = 0
        self.pre_rotation_gate_ready = True
        self.pre_rotation_gate_calls = 0

    def verify_pre_rotation_gate(self):
        self.pre_rotation_gate_calls += 1
        if not self.pre_rotation_gate_ready:
            raise MODULE.RolloverError("pre_rotation_dry_run_result_mismatch")

    def initial_context(self):
        return MODULE.InitialContext(
            self.old_url, self.role_name, self.chat_id, self.user_id
        )

    def credential_evidence(self, _state):
        if self.credential_mode == "rotated":
            return MODULE.CredentialEvidence(
                "rotated",
                "authenticated",
                "authentication_rejected",
                "authenticated",
            )
        if self.credential_mode == "unrotated":
            return MODULE.CredentialEvidence(
                "unrotated",
                "authentication_rejected",
                "authenticated",
                "authentication_rejected",
            )
        return MODULE.CredentialEvidence(
            "ambiguous", "tls_error", "transport_error", "tls_error"
        )

    def alter_password(self, _state):
        self.alter_calls += 1
        if self.alter_result == "unknown-committed":
            self.credential_mode = "rotated"
            raise MODULE.RolloverError("database_password_alter_result_unknown")
        if self.alter_result == "unknown-uncommitted":
            raise MODULE.RolloverError("database_password_alter_result_unknown")
        self.credential_mode = "rotated"

    def run_config(self, state, database_url, *, apply):
        self.config_calls.append((database_url, apply))
        if apply:
            if database_url is None:
                self.config_state = "disabled"
            else:
                self.config_url = database_url
                self.config_state = "direct"
                self.database_binding = (
                    "old" if database_url == self.old_url else "rotated"
                )
        return {
            "success": True,
            "action_status": "production_config_applied"
            if apply
            else "production_config_ready",
        }

    def configuration_matches(self, _state, database_url, desired_state):
        return self.config_url == database_url and self.config_state == desired_state

    def cleanup_config_stage_files(self):
        self.config_stage_cleanup_calls += 1
        return {
            "staged_secret_files_removed_count": 0,
            "staged_secret_files_absent": True,
        }

    def persistent_database_binding(self, _state):
        return self.database_binding

    def verify_runtime_reload(self, _state, database_url):
        if not self.runtime_ready or self.config_url != database_url:
            raise MODULE.RolloverError("core_service_reload_gate_failed")

    def apply_private_policy(self, _state):
        self.policy_calls += 1
        self.policy_applied = True
        return {
            "policy_digest": self.policy_digest,
            "policy_version": self.policy_version,
        }

    def policy_matches(self, state):
        return (
            self.policy_applied
            and state["policy_digest"] == self.policy_digest
            and state["policy_version"] == self.policy_version
        )


class SimulatedCrash(BaseException):
    pass


class XiaomanSharedDbPasswordRolloverTest(unittest.TestCase):
    def setUp(self) -> None:
        parent = "/private/tmp" if Path("/private/tmp").is_dir() else None
        self.temp = tempfile.TemporaryDirectory(
            prefix="qintopia-xiaoman-db-rollover-", dir=parent
        )
        self.root = Path(self.temp.name)
        self.owner_uid = os.geteuid()
        self.operation_id = str(uuid.uuid4())
        self.release_sha = "a" * 40
        self.dry_run_request_id = "deploy-20260803T000000Z-aaaaaaaaaaaa"
        self.old_url = (
            "postgresql://shared_role:old-password@db-a:5432,db-b:5432/"
            "qintopia?sslmode=verify-full&target_session_attrs=read-write"
        )
        self.approved = MODULE.ApprovedRequest(
            operation_id=self.operation_id,
            release_sha=self.release_sha,
            dry_run_request_id=self.dry_run_request_id,
            rollover_script_sha256="1" * 64,
            config_script_sha256="2" * 64,
            policy_script_sha256="3" * 64,
            old_database_url_sha256=sha256(self.old_url),
            role_ref=MODULE.opaque_ref(["postgres-role-v1", "shared_role"]),
            conversation_ref=MODULE.opaque_ref(
                ["conversation-ref-v3", "feishu", "oc_private"]
            ),
            actor_ref=MODULE.opaque_ref(
                ["poster-actor-v1", "feishu", "ou_requester"]
            ),
        )
        self.store = MODULE.StateStore(
            self.root / "persistent-state", owner_uid=self.owner_uid
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    def machine(self, operations, **overrides):
        options = {
            "password_factory": lambda: "N" * 64,
            "now": lambda: "2026-08-03T00:00:00+00:00",
            "boot_id_factory": lambda: "11111111-1111-1111-1111-111111111111",
            "monotonic_factory": lambda: 123456,
        }
        options.update(overrides)
        return MODULE.RolloverMachine(
            approved=self.approved,
            store=self.store,
            operations=operations,
            **options,
        )

    def write_env(self, path: Path, values: dict[str, str]) -> None:
        path.write_text(
            "".join(f"{name}='{value}'\n" for name, value in values.items()),
            encoding="utf-8",
        )
        path.chmod(0o600)

    def write_json(self, path: Path, value: dict, *, mode: int = 0o600) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
        path.chmod(mode)

    def write_pre_rotation_evidence(
        self, root: Path
    ) -> tuple[Path, Path, dict[str, Path]]:
        releases = root / "releases"
        release = releases / self.release_sha
        release.mkdir(parents=True)
        current = releases / "current"
        current.symlink_to(release)
        deploy_state = root / "deploy-state"
        files = {
            "manifest": release / "manifest.json",
            "request": (
                deploy_state
                / "requests/processed"
                / f"{self.dry_run_request_id}.json"
            ),
            "result": deploy_state / "results" / f"{self.dry_run_request_id}.json",
        }
        self.write_json(
            files["manifest"],
            {
                "schema_version": 2,
                "release_sha": self.release_sha,
                "runtime_sha": self.release_sha,
                "runtime_artifact_profile": "huabaosi-production",
                "deploy_bundle_sha": self.release_sha,
                "commit_sha": self.release_sha,
                "release_scope": list(MODULE.EXPECTED_RELEASE_SCOPE),
                "restart_targets": list(PRODUCTION_RELEASE_RESTART_TARGETS),
                "dry_run": False,
            },
        )
        self.write_json(
            files["request"],
            {
                "schema_version": 1,
                "request_id": self.dry_run_request_id,
                "environment": "production",
                "repository": "qintopia-agent-studio/qintopia-agent-os",
                "commit_sha": self.release_sha,
                "runtime_sha": self.release_sha,
                "runtime_artifact_profile": "huabaosi-production",
                "deploy_bundle_sha": self.release_sha,
                "release_sha": self.release_sha,
                "release_scope": list(MODULE.EXPECTED_RELEASE_SCOPE),
                "restart_targets": list(PRODUCTION_RELEASE_RESTART_TARGETS),
                "rollback_on_smoke_failure": True,
                "dry_run": True,
            },
        )
        self.write_json(
            files["result"],
            {
                "schema_version": 1,
                "request_id": self.dry_run_request_id,
                "environment": "production",
                "status": "dry_run_succeeded",
                "release_sha": self.release_sha,
                "commit_sha": self.release_sha,
                "runtime_sha": self.release_sha,
                "runtime_artifact_profile": "huabaosi-production",
                "deploy_bundle_sha": self.release_sha,
                "release_scope": list(MODULE.EXPECTED_RELEASE_SCOPE),
                "current_target": str(release.resolve()),
                "restart_targets": list(PRODUCTION_RELEASE_RESTART_TARGETS),
                "checks": [{"name": "deploy-runner", "status": "passed"}],
                "rollback": {"attempted": False, "status": "not_needed"},
            },
        )
        return current, deploy_state, files

    def test_pre_rotation_dry_run_gate_accepts_exact_protected_evidence(self) -> None:
        self.assertEqual(
            MODULE.EXPECTED_RESTART_TARGETS,
            PRODUCTION_RELEASE_RESTART_TARGETS,
        )
        current, deploy_state, _ = self.write_pre_rotation_evidence(
            self.root / "valid-gate"
        )
        MODULE.verify_pre_rotation_dry_run(
            release_current=current,
            deploy_state_root=deploy_state,
            approved=self.approved,
            owner_uid=self.owner_uid,
        )

    def test_pre_rotation_dry_run_gate_rejects_incomplete_or_mismatched_evidence(
        self,
    ) -> None:
        cases = [
            ("missing-manifest", "manifest", None, None),
            ("missing-request", "request", None, None),
            ("missing-result", "result", None, None),
            (
                "manifest-scope",
                "manifest",
                "release_scope",
                ["sidecar-runtime"],
            ),
            (
                "manifest-restart-order",
                "manifest",
                "restart_targets",
                list(reversed(PRODUCTION_RELEASE_RESTART_TARGETS)),
            ),
            ("manifest-not-live", "manifest", "dry_run", True),
            ("request-id", "request", "request_id", "deploy-invalid"),
            ("request-sha", "request", "release_sha", "b" * 40),
            (
                "request-profile",
                "request",
                "runtime_artifact_profile",
                "qiwe-production",
            ),
            (
                "request-scope",
                "request",
                "release_scope",
                ["sidecar-runtime"],
            ),
            (
                "request-restarts",
                "request",
                "restart_targets",
                ["qintopia-system-services"],
            ),
            (
                "request-restart-order",
                "request",
                "restart_targets",
                list(reversed(PRODUCTION_RELEASE_RESTART_TARGETS)),
            ),
            ("request-not-dry", "request", "dry_run", False),
            ("result-failed", "result", "status", "failed"),
            ("result-target", "result", "current_target", "/wrong/release"),
            (
                "result-restarts",
                "result",
                "restart_targets",
                ["hermes-erhua"],
            ),
            (
                "result-restart-order",
                "result",
                "restart_targets",
                list(reversed(PRODUCTION_RELEASE_RESTART_TARGETS)),
            ),
            (
                "result-rollback",
                "result",
                "rollback",
                {"attempted": True, "status": "succeeded"},
            ),
            (
                "result-check",
                "result",
                "checks",
                [{"name": "deploy-runner", "status": "skipped"}],
            ),
            ("result-no-checks", "result", "checks", []),
            (
                "result-duplicate-runner",
                "result",
                "checks",
                [
                    {"name": "deploy-runner", "status": "passed"},
                    {"name": "deploy-runner", "status": "passed"},
                ],
            ),
            ("result-error", "result", "error", "fixture failure"),
        ]
        for name, artifact, field, value in cases:
            with self.subTest(name=name):
                current, deploy_state, files = self.write_pre_rotation_evidence(
                    self.root / name
                )
                if field is None:
                    files[artifact].unlink()
                else:
                    payload = json.loads(files[artifact].read_text(encoding="utf-8"))
                    payload[field] = value
                    self.write_json(files[artifact], payload)
                with self.assertRaises(MODULE.RolloverError):
                    MODULE.verify_pre_rotation_dry_run(
                        release_current=current,
                        deploy_state_root=deploy_state,
                        approved=self.approved,
                        owner_uid=self.owner_uid,
                    )

    def test_failed_pre_rotation_gate_creates_no_state_or_password_change(self) -> None:
        operations = FakeOperations(self.approved)
        operations.pre_rotation_gate_ready = False
        password_calls = []
        machine = self.machine(
            operations,
            password_factory=lambda: password_calls.append(True) or "N" * 64,
        )
        with self.assertRaisesRegex(
            MODULE.RolloverError, "pre_rotation_dry_run_result_mismatch"
        ):
            machine.run("prepare")
        self.assertEqual(operations.pre_rotation_gate_calls, 1)
        self.assertEqual(operations.alter_calls, 0)
        self.assertEqual(operations.config_calls, [])
        self.assertEqual(password_calls, [])
        self.assertIsNone(self.store.read_state(self.operation_id))

    def test_pre_rotation_evidence_file_boundaries_fail_closed(self) -> None:
        cases = ["symlink", "writable", "oversized", "malformed", "duplicate-key"]
        for name in cases:
            with self.subTest(name=name):
                current, deploy_state, files = self.write_pre_rotation_evidence(
                    self.root / f"evidence-{name}"
                )
                request = files["request"]
                if name == "symlink":
                    target = request.with_name("outside.json")
                    target.write_text("{}\n", encoding="utf-8")
                    request.unlink()
                    request.symlink_to(target)
                elif name == "writable":
                    request.chmod(0o666)
                elif name == "oversized":
                    request.write_bytes(b"x" * (MODULE.MAX_STATE_BYTES + 1))
                    request.chmod(0o600)
                elif name == "malformed":
                    request.write_text("{\n", encoding="utf-8")
                    request.chmod(0o600)
                else:
                    request.write_text(
                        '{"schema_version":1,"schema_version":1}\n',
                        encoding="utf-8",
                    )
                    request.chmod(0o600)
                with self.assertRaises(MODULE.RolloverError):
                    MODULE.verify_pre_rotation_dry_run(
                        release_current=current,
                        deploy_state_root=deploy_state,
                        approved=self.approved,
                        owner_uid=self.owner_uid,
                    )

    def test_pre_rotation_evidence_wrong_owner_fails_closed(self) -> None:
        _, _, files = self.write_pre_rotation_evidence(self.root / "wrong-owner")
        metadata = files["request"].stat()
        wrong_owner = mock.Mock(
            st_mode=metadata.st_mode,
            st_uid=self.owner_uid + 1,
            st_size=metadata.st_size,
        )
        with mock.patch.object(MODULE.os, "fstat", return_value=wrong_owner):
            with self.assertRaises(MODULE.RolloverError):
                MODULE.read_protected_json(
                    files["request"], owner_uid=self.owner_uid
                )

    def test_approved_request_requires_valid_dry_run_request_id(self) -> None:
        payload = {"schema_version": 1, **self.approved.public_identity()}
        self.assertEqual(
            MODULE.load_approved_request(
                json.dumps(payload, separators=(",", ":")).encode("utf-8")
            ),
            self.approved,
        )
        for value in [None, "deploy-invalid", "deploy-20260803T000000Z-ABCDEF0"]:
            with self.subTest(value=value):
                invalid = dict(payload)
                invalid["dry_run_request_id"] = value
                with self.assertRaises(MODULE.RolloverError):
                    MODULE.load_approved_request(
                        json.dumps(invalid, separators=(",", ":")).encode("utf-8")
                    )

    def test_scram_sql_never_contains_plaintext_and_uri_routing_is_preserved(self) -> None:
        password = "new-plaintext-password-" + "x" * 48
        rotated = MODULE.rotated_database_url(self.old_url, password)
        self.assertIn("db-a:5432,db-b:5432", rotated)
        self.assertTrue(
            rotated.endswith(
                "?sslmode=verify-full&target_session_attrs=read-write"
            )
        )
        verifier = MODULE.scram_verifier(password)
        sql = MODULE.password_rotation_sql("shared_role", verifier)
        self.assertNotIn(password, verifier)
        self.assertNotIn(password, sql)
        self.assertIn("SET LOCAL synchronous_commit = on", sql)
        self.assertIn("SCRAM-SHA-256$4096:", sql)

    def test_psql_failure_classification_separates_auth_tls_and_transport(self) -> None:
        self.assertEqual(
            MODULE.classify_psql_failure(
                'psql: error: FATAL:  password authentication failed for user "shared_role"'
            ),
            "authentication_rejected",
        )
        self.assertEqual(
            MODULE.classify_psql_failure("psql: error: fe_sendauth: no password supplied"),
            "server_error",
        )
        self.assertEqual(
            MODULE.classify_psql_failure("psql: error: SSL error: certificate verify failed"),
            "tls_error",
        )
        self.assertEqual(
            MODULE.classify_psql_failure("psql: error: connection timed out"),
            "transport_error",
        )
        self.assertEqual(
            MODULE.classify_psql_failure("psql: error: database does not exist"),
            "server_error",
        )

    def test_prepare_reconciles_unknown_alter_commit_and_persists_root_only_state(self) -> None:
        operations = FakeOperations(self.approved, alter_result="unknown-committed")
        report = self.machine(operations).run("prepare")
        self.assertEqual(report["phase"], "direct_config_applied")
        self.assertEqual(report["credential_state"], "rotated")
        self.assertTrue(report["reload_required"])
        state_path = self.store.state_path(self.operation_id)
        self.assertTrue(state_path.is_file())
        self.assertEqual(stat.S_IMODE(self.store.root.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE(state_path.stat().st_mode), 0o600)
        self.assertFalse(str(self.store.root).startswith("/run/"))
        state = json.loads(state_path.read_text(encoding="utf-8"))
        self.assertEqual(state["phase"], "direct_config_applied")
        self.assertIn("new_url", state)
        self.assertEqual(
            state["previous_database_url_sha256"],
            self.approved.old_database_url_sha256,
        )

    def test_mixed_old_new_configuration_converges_but_third_value_fails_closed(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        state = machine._new_state()
        machine._update(state, "preview_validated")
        machine._update(state, "alter_in_flight")
        operations.credential_mode = "rotated"
        operations.database_binding = "mixed"
        operations.config_state = "mixed"
        report = self.machine(operations).run("prepare")
        self.assertEqual(report["phase"], "direct_config_applied")
        self.assertEqual(operations.database_binding, "rotated")
        self.assertTrue(report["previous_database_url_sha256_matched"])

        other_store = MODULE.StateStore(
            self.root / "other-persistent-state", owner_uid=self.owner_uid
        )
        other_operations = FakeOperations(self.approved)
        other_machine = MODULE.RolloverMachine(
            approved=self.approved,
            store=other_store,
            operations=other_operations,
            password_factory=lambda: "N" * 64,
            now=lambda: "2026-08-03T00:00:00+00:00",
            boot_id_factory=lambda: "11111111-1111-1111-1111-111111111111",
            monotonic_factory=lambda: 123456,
        )
        other_state = other_machine._new_state()
        other_machine._update(other_state, "preview_validated")
        other_machine._update(other_state, "alter_in_flight")
        other_operations.credential_mode = "rotated"
        other_operations.database_binding = "other"
        applied_before = len(
            [call for call in other_operations.config_calls if call[1]]
        )
        with self.assertRaisesRegex(
            MODULE.RolloverError, "unexpected_database_configuration_binding"
        ):
            other_machine.run("prepare")
        self.assertEqual(
            len([call for call in other_operations.config_calls if call[1]]),
            applied_before,
        )

    def test_prepare_process_crash_reconciles_from_alter_in_flight(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        state = machine._new_state()
        machine._update(state, "preview_validated")
        machine._update(state, "alter_in_flight")
        operations.pre_rotation_gate_ready = False
        operations.credential_mode = "rotated"
        report = self.machine(operations).run("prepare")
        self.assertEqual(report["phase"], "direct_config_applied")
        self.assertEqual(len([call for call in operations.config_calls if call[1]]), 1)
        self.assertEqual(operations.pre_rotation_gate_calls, 0)

    def test_persistent_state_is_reentrant_after_process_and_boot_restart(self) -> None:
        operations = FakeOperations(self.approved)
        first = self.machine(operations)
        first.run("prepare")
        restarted_store = MODULE.StateStore(
            self.store.root, owner_uid=self.owner_uid
        )
        restarted = MODULE.RolloverMachine(
            approved=self.approved,
            store=restarted_store,
            operations=operations,
            password_factory=lambda: "unused" * 16,
            now=lambda: "2026-08-03T00:01:00+00:00",
            boot_id_factory=lambda: "22222222-2222-2222-2222-222222222222",
            monotonic_factory=lambda: 100,
        )
        report = restarted.run("verify-reload")
        self.assertEqual(report["phase"], "reload_verified")

    def test_post_policy_rollback_keeps_valid_rotated_credential_and_disables_poster(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        machine.run("prepare")
        machine.run("verify-reload")
        machine.run("apply-private-policy")
        report = machine.run("rollback")
        self.assertEqual(report["phase"], "rollback_config_applied")
        self.assertEqual(operations.config_state, "disabled")
        self.assertEqual(operations.credential_mode, "rotated")
        terminal = machine.run("rollback-verify")
        self.assertEqual(
            terminal["action_status"], "password_rollover_rollback_completed"
        )
        self.assertEqual(terminal["credential_binding"], "rotated")
        self.assertEqual(
            terminal["active_database_url_sha256"],
            sha256(operations.config_url),
        )
        self.assertFalse(self.store.state_path(self.operation_id).exists())
        self.assertTrue(self.store.receipt_path(self.operation_id).exists())

    def test_unknown_uncommitted_alter_can_abort_with_old_credential(self) -> None:
        operations = FakeOperations(self.approved, alter_result="unknown-uncommitted")
        machine = self.machine(operations)
        state = machine._new_state()
        machine._update(state, "preview_validated")
        machine._update(state, "alter_in_flight")
        terminal = machine.run("rollback")
        self.assertEqual(terminal["action_status"], "password_rollover_aborted")
        self.assertEqual(terminal["credential_binding"], "old")
        self.assertEqual(
            terminal["active_database_url_sha256"],
            self.approved.old_database_url_sha256,
        )

    def test_terminal_receipt_precedes_secret_cleanup_and_recovers_after_crash(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        machine.run("prepare")
        machine.run("verify-reload")
        machine.run("apply-private-policy")

        orphan = self.store.root / f".{self.operation_id}.state.json.abcdefgh.tmp"

        def crash_after_receipt():
            orphan.write_text(
                json.dumps(
                    {
                        "old_url": operations.old_url,
                        "new_url": operations.config_url,
                    }
                ),
                encoding="utf-8",
            )
            orphan.chmod(0o600)
            raise SimulatedCrash()

        with self.assertRaises(SimulatedCrash):
            self.machine(operations, terminal_hook=crash_after_receipt).run(
                "forward-verify"
            )
        self.assertTrue(self.store.receipt_path(self.operation_id).exists())
        self.assertTrue(self.store.state_path(self.operation_id).exists())
        self.assertTrue(orphan.exists())
        receipt = json.loads(
            self.store.receipt_path(self.operation_id).read_text(encoding="utf-8")
        )
        self.assertFalse(receipt["secret_cleanup_completed"])
        self.assertEqual(
            receipt["previous_database_url_sha256"],
            self.approved.old_database_url_sha256,
        )
        self.assertEqual(
            receipt["new_database_url_sha256"],
            sha256(operations.config_url),
        )

        recovered = self.machine(operations).run("forward-verify")
        self.assertTrue(recovered["deduped"])
        self.assertTrue(recovered["secret_state_removed"])
        self.assertFalse(self.store.state_path(self.operation_id).exists())
        self.assertFalse(orphan.exists())
        self.store.assert_secret_state_removed(self.operation_id)
        self.assertGreaterEqual(operations.config_stage_cleanup_calls, 2)

    def test_sigkill_during_secret_state_replace_cleans_orphan_on_restart(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        canonical = machine._new_state()
        child = """
import importlib.util
import os
import signal
import sys
from pathlib import Path

sys.dont_write_bytecode = True
script_path, state_root, operation_id = sys.argv[1:4]
spec = importlib.util.spec_from_file_location("sigkill_rollover", script_path)
if spec is None or spec.loader is None:
    raise SystemExit(2)
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
store = module.StateStore(Path(state_root), owner_uid=os.geteuid())
state = store.read_state(operation_id)
if state is None:
    raise SystemExit(3)
state["updated_at"] = "2026-08-03T00:00:01+00:00"
module.os.replace = lambda _source, _target: os.kill(os.getpid(), signal.SIGKILL)
store.write_state(state)
"""
        result = subprocess.run(
            [
                sys.executable,
                "-c",
                child,
                str(SCRIPT_PATH),
                str(self.store.root),
                self.operation_id,
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            check=False,
            timeout=5,
            env={"PATH": "/usr/bin:/bin", "PYTHONDONTWRITEBYTECODE": "1"},
        )
        self.assertEqual(result.returncode, -signal.SIGKILL)
        orphans = [
            path
            for path in self.store.root.iterdir()
            if MODULE.STATE_TEMP_NAME_RE.fullmatch(path.name)
        ]
        self.assertEqual(len(orphans), 1)
        orphan_payload = orphans[0].read_bytes()
        self.assertIn(canonical["old_url"].encode("utf-8"), orphan_payload)
        self.assertIn(canonical["new_url"].encode("utf-8"), orphan_payload)

        restarted = MODULE.RolloverMachine(
            approved=self.approved,
            store=MODULE.StateStore(self.store.root, owner_uid=self.owner_uid),
            operations=operations,
            password_factory=lambda: "unused" * 16,
        )
        report = restarted.run("status")
        self.assertEqual(report["phase"], "escrowed")
        self.assertFalse(orphans[0].exists())
        self.assertEqual(self.store.read_state(self.operation_id), canonical)

    def test_unsafe_orphaned_state_record_fails_before_terminal_reconciliation(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        machine.run("prepare")
        orphan = self.store.root / f".{self.operation_id}.state.json.abcdefgh.tmp"
        orphan.symlink_to(self.store.state_path(self.operation_id))
        with self.assertRaisesRegex(
            MODULE.RolloverError, "rollover_temporary_record_boundary_invalid"
        ):
            machine.run("verify-reload")
        self.assertTrue(orphan.is_symlink())

    def test_terminal_receipt_rejects_database_identity_tampering(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        machine.run("prepare")
        machine.run("verify-reload")
        machine.run("apply-private-policy")

        def crash_after_receipt():
            raise SimulatedCrash()

        with self.assertRaises(SimulatedCrash):
            self.machine(operations, terminal_hook=crash_after_receipt).run(
                "forward-verify"
            )
        receipt = json.loads(
            self.store.receipt_path(self.operation_id).read_text(encoding="utf-8")
        )
        receipt["previous_database_url_sha256"] = "f" * 64
        self.store.write_receipt(receipt)
        with self.assertRaisesRegex(
            MODULE.RolloverError, "rollover_record_database_identity_mismatch"
        ):
            self.machine(operations).run("forward-verify")

    def test_production_reload_selects_opposite_credential_as_retired(self) -> None:
        paths = MODULE.RuntimePaths(
            release_current=self.root / "current",
            sidecar_env=self.root / "sidecar.env",
            hermes_env=self.root / "hermes.env",
            erhua_env=self.root / "erhua.env",
            state_root=self.store.root,
            self_path=SCRIPT_PATH,
        )
        operations = MODULE.ProductionOperations(
            paths=paths,
            approved=self.approved,
            config_script=self.root / "config.py",
            policy_script=self.root / "policy.py",
        )
        new_url = MODULE.rotated_database_url(self.old_url, "N" * 64)
        state = {
            "old_url": self.old_url,
            "new_url": new_url,
            "config_applied_boot_id": "11111111-1111-1111-1111-111111111111",
            "config_applied_monotonic_us": 123456,
        }
        properties = {
            "ActiveState": "active",
            "MainPID": "42",
            "ExecMainStartTimestampMonotonic": "123457",
            "Result": "success",
            "ExecMainStatus": "0",
        }
        with (
            mock.patch.object(MODULE, "boot_id", return_value=state["config_applied_boot_id"]),
            mock.patch.object(MODULE, "systemd_properties", return_value=properties),
            mock.patch.object(
                MODULE,
                "process_runtime_binding",
                return_value=(sha256(self.old_url), self.release_sha),
            ),
            mock.patch.object(
                operations,
                "cleanup_config_stage_files",
                return_value={"staged_secret_files_absent": True},
            ),
            mock.patch.object(MODULE, "verify_retired_process_credential") as retired,
        ):
            operations.verify_runtime_reload(state, self.old_url)
        retired.assert_called_once_with(
            new_url, self.old_url, minimum_new=len(MODULE.CORE_SERVICES)
        )

    def test_production_operations_rollback_old_credential_from_mixed_config(self) -> None:
        sidecar = self.root / "rollback-sidecar.env"
        hermes = self.root / "rollback-xiaoman.env"
        erhua = self.root / "rollback-erhua.env"
        paths = MODULE.RuntimePaths(
            release_current=self.root / "current",
            sidecar_env=sidecar,
            hermes_env=hermes,
            erhua_env=erhua,
            state_root=self.store.root,
            self_path=SCRIPT_PATH,
        )
        operations = MODULE.ProductionOperations(
            paths=paths,
            approved=self.approved,
            config_script=self.root / "config.py",
            policy_script=self.root / "policy.py",
        )

        def write_configuration(database_url: str, desired_state: str) -> None:
            database_hash = sha256(database_url)
            enabled = "1" if desired_state == "direct" else "0"
            ingress_key = "I" * 48
            callback_key = "C" * 48
            sidecar_values = {
                "QINTOPIA_SIDECAR_DATABASE_URL": database_url,
                "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED": enabled,
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": enabled,
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY": ingress_key,
                "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY": callback_key,
                "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
                "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS": "oc_private",
                "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS": "ou_requester",
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS": "oc_private",
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS": "ou_requester",
            }
            sidecar_values.update(
                {name: database_hash for name in MODULE.DATABASE_HASH_KEYS}
            )
            self.write_env(sidecar, sidecar_values)
            self.write_env(
                hermes,
                {
                    "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE": enabled,
                    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": enabled,
                    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY": ingress_key,
                    "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY": callback_key,
                    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
                },
            )
            self.write_env(
                erhua, {"QINTOPIA_SIDECAR_DATABASE_URL": database_url}
            )

        write_configuration(self.old_url, "disabled")
        machine = self.machine(operations)
        state = machine._new_state()
        new_url = state["new_url"]
        new_hash = state["new_database_url_sha256"]
        machine._update(state, "preview_validated")

        mixed_values = MODULE.parse_env_values(
            sidecar,
            set(MODULE.DATABASE_HASH_KEYS) | {"QINTOPIA_SIDECAR_DATABASE_URL"},
        )
        mixed_values[MODULE.DATABASE_HASH_KEYS[0]] = new_hash
        self.write_env(sidecar, mixed_values)
        self.write_env(erhua, {"QINTOPIA_SIDECAR_DATABASE_URL": new_url})
        self.assertEqual(operations.persistent_database_binding(state), "mixed")

        config_targets: list[str | None] = []

        def apply_config(_state, database_url, *, apply):
            self.assertTrue(apply)
            config_targets.append(database_url)
            write_configuration(database_url or self.old_url, "direct" if database_url else "disabled")
            return {"success": True, "action_status": "production_config_applied"}

        properties = {
            "ActiveState": "active",
            "MainPID": "42",
            "ExecMainStartTimestampMonotonic": "123457",
            "Result": "success",
            "ExecMainStatus": "0",
        }
        unrotated = MODULE.CredentialEvidence(
            "unrotated",
            "authentication_rejected",
            "authenticated",
            "authentication_rejected",
        )
        with (
            mock.patch.object(
                operations, "credential_evidence", return_value=unrotated
            ),
            mock.patch.object(operations, "run_config", side_effect=apply_config),
        ):
            report = machine.run("rollback")
        self.assertEqual(report["phase"], "rollback_config_applied")
        self.assertEqual(config_targets, [self.old_url, None])
        rollback_state = self.store.read_state(self.operation_id)
        self.assertIsNotNone(rollback_state)
        self.assertTrue(
            operations.configuration_matches(rollback_state, self.old_url, "disabled")
        )

        with (
            mock.patch.object(
                operations, "credential_evidence", return_value=unrotated
            ),
            mock.patch.object(operations, "cleanup_config_stage_files"),
            mock.patch.object(
                MODULE,
                "boot_id",
                return_value=rollback_state["config_applied_boot_id"],
            ),
            mock.patch.object(MODULE, "systemd_properties", return_value=properties),
            mock.patch.object(
                MODULE,
                "process_runtime_binding",
                return_value=(sha256(self.old_url), self.release_sha),
            ),
            mock.patch.object(MODULE, "verify_retired_process_credential") as retired,
        ):
            terminal = machine.run("rollback-verify")
        self.assertEqual(
            terminal["action_status"], "password_rollover_rollback_completed"
        )
        self.assertEqual(terminal["credential_binding"], "old")
        retired.assert_called_once_with(
            new_url, self.old_url, minimum_new=len(MODULE.CORE_SERVICES)
        )

    def test_exact_operator_binding_rejects_wrong_database_role_chat_and_actor(self) -> None:
        sidecar = self.root / "message-sidecar.env"
        hermes = self.root / "xiaoman.env"
        sidecar.write_text(
            "\n".join(
                [
                    f"QINTOPIA_SIDECAR_DATABASE_URL='{self.old_url}'",
                    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS='oc_private'",
                    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS='ou_requester'",
                ]
            )
            + "\n",
            encoding="utf-8",
        )
        sidecar.chmod(0o600)
        hermes.write_text("# fixture\n", encoding="utf-8")
        hermes.chmod(0o600)
        paths = MODULE.RuntimePaths(
            release_current=self.root / "current",
            sidecar_env=sidecar,
            hermes_env=hermes,
            erhua_env=self.root / "erhua.env",
            state_root=self.store.root,
            self_path=SCRIPT_PATH,
        )
        for field in [
            "old_database_url_sha256",
            "role_ref",
            "conversation_ref",
            "actor_ref",
        ]:
            values = dict(self.approved.__dict__)
            values[field] = (
                "f" * 64 if field.endswith("sha256") else "sha256:" + "f" * 64
            )
            operations = MODULE.ProductionOperations(
                paths=paths,
                approved=MODULE.ApprovedRequest(**values),
                config_script=self.root / "config.py",
                policy_script=self.root / "policy.py",
            )
            with self.subTest(field=field), self.assertRaises(MODULE.RolloverError):
                operations.initial_context()

    def test_persisted_state_rederives_targets_and_password_only_rotation(self) -> None:
        operations = FakeOperations(self.approved)
        machine = self.machine(operations)
        original = machine._new_state()
        mutations = {
            "dry_run_request_id": "deploy-20260803T000001Z-bbbbbbbbbbbb",
            "role_name": "other_role",
            "chat_id": "oc_other",
            "user_id": "ou_other",
            "new_url": original["new_url"].replace("db-a:5432", "db-c:5432"),
        }
        for field, value in mutations.items():
            with self.subTest(field=field):
                tampered = dict(original)
                tampered[field] = value
                if field == "new_url":
                    tampered["new_database_url_sha256"] = sha256(value)
                self.store.write_state(tampered)
                with self.assertRaises(MODULE.RolloverError):
                    machine.run("status")
                self.store.write_state(original)

    def test_production_operations_bind_payload_and_reconciliation_to_erhua(self) -> None:
        sidecar = self.root / "message-sidecar.env"
        hermes = self.root / "xiaoman.env"
        erhua = self.root / "erhua.env"
        paths = MODULE.RuntimePaths(
            release_current=self.root / "current",
            sidecar_env=sidecar,
            hermes_env=hermes,
            erhua_env=erhua,
            state_root=self.store.root,
            self_path=SCRIPT_PATH,
        )
        operations = MODULE.ProductionOperations(
            paths=paths,
            approved=self.approved,
            config_script=self.root / "config.py",
            policy_script=self.root / "policy.py",
        )
        new_url = MODULE.rotated_database_url(self.old_url, "N" * 64)
        new_hash = sha256(new_url)
        state = {
            "old_url": self.old_url,
            "new_url": new_url,
            "new_database_url_sha256": new_hash,
            "chat_id": "oc_private",
            "user_id": "ou_requester",
        }

        forward_payload = json.loads(operations._config_payload(state, new_url))
        rollback_payload = json.loads(operations._config_payload(state, self.old_url))
        self.assertEqual(
            forward_payload["previous_database_url_sha256"], sha256(self.old_url)
        )
        self.assertEqual(
            rollback_payload["previous_database_url_sha256"], new_hash
        )

        old_hash = sha256(self.old_url)
        old_values = {"QINTOPIA_SIDECAR_DATABASE_URL": self.old_url}
        old_values.update({name: old_hash for name in MODULE.DATABASE_HASH_KEYS})
        self.write_env(sidecar, old_values)
        self.write_env(erhua, {"QINTOPIA_SIDECAR_DATABASE_URL": self.old_url})
        self.assertEqual(operations.persistent_database_binding(state), "old")
        self.write_env(erhua, {"QINTOPIA_SIDECAR_DATABASE_URL": new_url})
        self.assertEqual(operations.persistent_database_binding(state), "mixed")
        self.write_env(
            erhua,
            {
                "QINTOPIA_SIDECAR_DATABASE_URL": (
                    "postgresql://shared_role:third@db-a:5432,db-b:5432/"
                    "qintopia?sslmode=verify-full&target_session_attrs=read-write"
                )
            },
        )
        self.assertEqual(operations.persistent_database_binding(state), "other")

        ingress_key = "I" * 48
        callback_key = "C" * 48
        direct_values = {
            "QINTOPIA_SIDECAR_DATABASE_URL": new_url,
            "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED": "1",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "1",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY": ingress_key,
            "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY": callback_key,
            "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
            "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS": "oc_private",
            "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS": "ou_requester",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS": "oc_private",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS": "ou_requester",
        }
        direct_values.update({name: new_hash for name in MODULE.DATABASE_HASH_KEYS})
        self.write_env(sidecar, direct_values)
        self.write_env(
            hermes,
            {
                "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE": "1",
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "1",
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY": ingress_key,
                "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY": callback_key,
                "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
            },
        )
        self.write_env(erhua, {"QINTOPIA_SIDECAR_DATABASE_URL": self.old_url})
        self.assertFalse(operations.configuration_matches(state, new_url, "direct"))
        self.write_env(erhua, {"QINTOPIA_SIDECAR_DATABASE_URL": new_url})
        self.assertTrue(operations.configuration_matches(state, new_url, "direct"))

        disabled_values = {
            "QINTOPIA_SIDECAR_DATABASE_URL": new_url,
            "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED": "0",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "0",
            "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
        }
        disabled_values.update(
            {name: new_hash for name in MODULE.DATABASE_HASH_KEYS}
        )
        self.write_env(sidecar, disabled_values)
        self.write_env(
            hermes,
            {
                "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE": "0",
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "0",
                "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
            },
        )
        self.assertTrue(operations.configuration_matches(state, new_url, "disabled"))

    def test_production_config_stage_cleanup_is_release_bound_and_redacted(self) -> None:
        paths = MODULE.RuntimePaths(
            release_current=self.root / "current",
            sidecar_env=self.root / "sidecar.env",
            hermes_env=self.root / "xiaoman.env",
            erhua_env=self.root / "erhua.env",
            state_root=self.store.root,
            self_path=SCRIPT_PATH,
        )
        operations = MODULE.ProductionOperations(
            paths=paths,
            approved=self.approved,
            config_script=self.root / "config.py",
            policy_script=self.root / "policy.py",
        )
        evidence = {
            "success": True,
            "action_status": "production_config_stage_cleanup_completed",
            "release_sha": self.release_sha,
            "staged_secret_files_removed_count": 2,
            "staged_secret_files_absent": True,
            "external_calls_executed": False,
            "database_writes_executed": False,
            "service_changes_executed": False,
            "sensitive_values_redacted": True,
        }
        completed = MODULE.subprocess.CompletedProcess(
            args=[],
            returncode=0,
            stdout=(
                "xiaoman_feishu_production_config="
                + json.dumps(evidence, separators=(",", ":"))
            ).encode("utf-8"),
            stderr=b"",
        )
        with mock.patch.object(
            operations, "_run_protected", return_value=completed
        ) as protected:
            report = operations.cleanup_config_stage_files()
        self.assertEqual(report["staged_secret_files_removed_count"], 2)
        protected.assert_called_once_with(
            self.root / "config.py",
            [
                "--cleanup-staged-files",
                "--release-sha",
                self.release_sha,
                "--approval",
                MODULE.CONFIG_APPROVAL,
            ],
            b"",
            timeout=30,
        )

    def test_protected_python_children_disable_bytecode_writes(self) -> None:
        paths = MODULE.RuntimePaths(
            release_current=self.root / "current",
            sidecar_env=self.root / "sidecar.env",
            hermes_env=self.root / "xiaoman.env",
            erhua_env=self.root / "erhua.env",
            state_root=self.store.root,
            self_path=SCRIPT_PATH,
        )
        operations = MODULE.ProductionOperations(
            paths=paths,
            approved=self.approved,
            config_script=self.root / "config.py",
            policy_script=self.root / "policy.py",
        )
        completed = MODULE.subprocess.CompletedProcess(
            args=[], returncode=0, stdout=b"", stderr=b""
        )
        with mock.patch.object(
            MODULE.subprocess, "run", return_value=completed
        ) as run:
            operations._run_protected(
                self.root / "policy.py", ["--stdin"], b"{}", timeout=1
            )
        self.assertEqual(
            run.call_args.kwargs["env"],
            {"PATH": "/usr/bin:/bin", "PYTHONDONTWRITEBYTECODE": "1"},
        )

    def test_release_boundary_rejects_symlink_writable_and_digest_drift(self) -> None:
        releases = self.root / "releases"
        release = releases / self.release_sha
        scripts = release / "deploy/sidecar/scripts"
        scripts.mkdir(parents=True)
        files = {}
        for relative, content in [
            (MODULE.SCRIPT_RELATIVE_PATH, b"#!/bin/sh\n"),
            (MODULE.CONFIG_SCRIPT_RELATIVE_PATH, b"#!/bin/sh\n# config\n"),
            (MODULE.POLICY_SCRIPT_RELATIVE_PATH, b"#!/bin/sh\n# policy\n"),
        ]:
            path = release / relative
            path.write_bytes(content)
            path.chmod(0o755)
            files[relative] = (path, sha256(content))
        current = releases / "current"
        current.symlink_to(release)
        approved = MODULE.ApprovedRequest(
            **{
                **self.approved.__dict__,
                "rollover_script_sha256": files[MODULE.SCRIPT_RELATIVE_PATH][1],
                "config_script_sha256": files[MODULE.CONFIG_SCRIPT_RELATIVE_PATH][1],
                "policy_script_sha256": files[MODULE.POLICY_SCRIPT_RELATIVE_PATH][1],
            }
        )
        paths = MODULE.RuntimePaths(
            release_current=current,
            sidecar_env=self.root / "sidecar.env",
            hermes_env=self.root / "hermes.env",
            erhua_env=self.root / "erhua.env",
            state_root=self.store.root,
            self_path=files[MODULE.SCRIPT_RELATIVE_PATH][0],
        )
        MODULE.verify_release_boundary(paths, approved, owner_uid=self.owner_uid)

        config_path = files[MODULE.CONFIG_SCRIPT_RELATIVE_PATH][0]
        config_path.chmod(0o775)
        with self.assertRaises(MODULE.RolloverError):
            MODULE.verify_release_boundary(paths, approved, owner_uid=self.owner_uid)
        config_path.chmod(0o755)
        config_path.write_text("digest drift\n", encoding="utf-8")
        with self.assertRaises(MODULE.RolloverError):
            MODULE.verify_release_boundary(paths, approved, owner_uid=self.owner_uid)

    def test_cli_surface_has_no_external_delivery_or_service_activation(self) -> None:
        script = SCRIPT_PATH.read_text(encoding="utf-8")
        self.assertIn(str(MODULE.STATE_ROOT_PATH), script)
        self.assertNotIn('Path("/run/', script)
        for forbidden in [
            "open-apis",
            "send_as_bot",
            "reply_in_thread",
            '"start",',
            '"restart",',
            '"enable",',
            "enable --now",
            "curl ",
        ]:
            self.assertNotIn(forbidden, script)


if __name__ == "__main__":
    unittest.main(verbosity=2)
