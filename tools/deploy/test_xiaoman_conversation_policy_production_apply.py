#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = (
    REPO_ROOT
    / "deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py"
)
SPEC = importlib.util.spec_from_file_location("xiaoman_policy_apply", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load Xiaoman policy apply module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def sha256(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


class XiaomanConversationPolicyApplyTest(unittest.TestCase):
    def setUp(self) -> None:
        parent = "/private/tmp" if Path("/private/tmp").is_dir() else None
        self.temp = tempfile.TemporaryDirectory(
            prefix="qintopia-xiaoman-policy-apply-", dir=parent
        )
        self.root = Path(self.temp.name)
        self.release_sha = "b" * 40
        self.release_root = self.root / self.release_sha
        self.sidecar_dir = self.release_root / "sidecar"
        self.sidecar_dir.mkdir(parents=True)
        self.binary = self.sidecar_dir / "qintopia-message-sidecar"
        self.release_current = self.root / "current"
        self.release_current.symlink_to(self.release_root)
        self.env_path = self.root / "message-sidecar.env"
        self.database_url = (
            "postgresql://policy_user:secret@db.example.internal:5432/qintopia"
        )
        self.database_hash = sha256(self.database_url)
        self.chat_id = "oc_private"
        self.user_id = "ou_requester"
        self.write_env(self.database_hash)
        self.write_fake_binary()
        self.release_root.chmod(0o555)
        self.body = json.dumps(
            {
                "schema_version": 3,
                "policies": [
                    {
                        "platform": "feishu",
                        "chat_id": self.chat_id,
                        "conversation_type": "direct",
                        "audience_class": "private",
                        "allowed_capabilities": [
                            "poster_production_request",
                            "poster_workflow_status",
                        ],
                        "return_mode": "direct_chat",
                        "initiation_rule": "direct_message",
                        "status_visibility": "requester",
                        "enabled": True,
                        "reviewer_user_ids": [],
                    }
                ],
            },
            separators=(",", ":"),
        ).encode("utf-8")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_env(self, database_hash: str) -> None:
        self.env_path.write_text(
            "\n".join(
                [
                    f"QINTOPIA_SIDECAR_DATABASE_URL='{self.database_url}'",
                    f"QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256='{database_hash}'",
                    f"QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS='{self.chat_id}'",
                    f"QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS='{self.user_id}'",
                    "QINTOPIA_UNRELATED_RUNTIME_SECRET='must-not-reach-child'",
                ]
            )
            + "\n",
            encoding="utf-8",
        )
        self.env_path.chmod(0o640)

    def write_fake_binary(self, *, disclose=False, fail=False) -> None:
        self.release_root.chmod(0o755)
        policy = {
            "conversation_ref": "sha256:" + "c" * 64,
            "policy_digest": "sha256:" + "d" * 64,
            "policy_version": 1,
            "enabled": True,
            "deduped": False,
            "reviewer_count": 0,
        }
        disclosure_field = (
            f",'fixture_disclosure': {self.chat_id!r}" if disclose else ""
        )
        self.binary.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "assert sys.argv[1:] == ['conversation-policy-apply', '--stdin']\n"
            "assert 'QINTOPIA_UNRELATED_RUNTIME_SECRET' not in os.environ\n"
            "assert os.environ['QINTOPIA_XIAOMAN_CONVERSATION_POLICY_APPROVAL'] == "
            f"{MODULE.APPLY_APPROVAL!r}\n"
            "payload = json.load(sys.stdin)\n"
            "assert payload['schema_version'] == 3\n"
            + ("print('fixture failure', file=sys.stderr)\nsys.exit(9)\n" if fail else "")
            + "print(json.dumps({"
            "'success': True,"
            "'action_status': 'conversation_policies_applied',"
            "'input_count': len(payload['policies']),"
            "'created_version_count': 1,"
            "'deduped_count': 0,"
            f"'policies': {[policy]!r},"
            f"'database_url_sha256': {self.database_hash!r},"
            "'approved_database_url_sha256_matched': True,"
            "'external_calls_executed': False,"
            "'sensitive_fields_redacted': True"
            + disclosure_field
            + "}))\n",
            encoding="utf-8",
        )
        self.binary.chmod(0o755)
        self.release_root.chmod(0o555)

    def run_apply(self) -> str:
        return MODULE.run_policy_apply(
            body=self.body,
            env_path=self.env_path,
            release_current_path=self.release_current,
            approval=MODULE.APPLY_APPROVAL,
            effective_uid=0,
        )

    def test_fixed_release_policy_apply_uses_minimal_environment_and_redacted_output(self) -> None:
        output = self.run_apply()
        report = json.loads(output)
        self.assertTrue(report["success"])
        self.assertFalse(report["external_calls_executed"])
        for sensitive in [
            self.database_url,
            self.chat_id,
            self.user_id,
            str(self.root),
            "must-not-reach-child",
        ]:
            self.assertNotIn(sensitive, output)

    def test_approval_database_and_output_boundaries_fail_closed(self) -> None:
        with self.assertRaises(MODULE.PolicyApplyError):
            MODULE.run_policy_apply(
                body=self.body,
                env_path=self.env_path,
                release_current_path=self.release_current,
                approval="",
                effective_uid=0,
            )
        with self.assertRaises(MODULE.PolicyApplyError):
            MODULE.run_policy_apply(
                body=self.body,
                env_path=self.env_path,
                release_current_path=self.release_current,
                approval=MODULE.APPLY_APPROVAL,
                effective_uid=501,
            )

        self.write_env("c" * 64)
        with self.assertRaises(MODULE.PolicyApplyError):
            self.run_apply()
        self.write_env(self.database_hash)

        self.write_fake_binary(disclose=True)
        with self.assertRaises(MODULE.PolicyApplyError):
            self.run_apply()
        self.write_fake_binary(fail=True)
        with self.assertRaises(MODULE.PolicyApplyError):
            self.run_apply()

    def test_input_and_cli_surface_are_bounded(self) -> None:
        with self.assertRaises(MODULE.PolicyApplyError):
            MODULE.load_policy_input(
                b'{"schema_version":3,"schema_version":3,"policies":[]}'
            )
        with self.assertRaises(MODULE.PolicyApplyError):
            MODULE.load_policy_input(b"{" + b"x" * MODULE.MAX_INPUT_BYTES + b"}")
        script = SCRIPT_PATH.read_text(encoding="utf-8")
        for required in [
            'SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")',
            'RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")',
            MODULE.APPLY_APPROVAL,
            '[str(binary), "conversation-policy-apply", "--stdin"]',
            '"PATH": "/usr/bin:/bin"',
        ]:
            self.assertIn(required, script)
        for forbidden in ["--test-mode", "--output", "systemctl", "curl ", "psql "]:
            self.assertNotIn(forbidden, script)


if __name__ == "__main__":
    unittest.main(verbosity=2)
