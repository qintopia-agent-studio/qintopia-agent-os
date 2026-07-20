import asyncio
import hashlib
import json
import os
import sys
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest import mock

import image_callback_bridge
from image_callback_bridge import (
    MAX_REPORT_BYTES,
    PROCESSOR_ENV_ALLOWLIST,
    PRODUCTION_PROCESSOR_ENV_ALLOWLIST,
    CallbackBridgeResult,
    QiWeImageCallbackBridge,
    is_async_image_callback,
)


def callback_body() -> bytes:
    return json.dumps(
        {
            "code": 0,
            "data": [
                {
                    "requestId": "raw-request-secret",
                    "cmd": 20000,
                    "msgData": {
                        "fileAesKey": "raw-aes-secret",
                        "fileId": "raw-file-secret",
                        "fileMd5": "98e7c2acf4391f8b4a2bbd39e364c5e3",
                        "fileSize": 48300,
                        "filename": "private-activity-poster.jpg",
                    },
                }
            ],
        }
    ).encode("utf-8")


def callback_report(**overrides):
    report = {
        "success": True,
        "dry_run": False,
        "apply_requested": True,
        "worker": "qiwe-image-send-adapter",
        "phase": "callback",
        "action_status": "image_send_completed",
        "work_item_id": str(uuid.uuid4()),
        "external_upload_requested": False,
        "callback_received": True,
        "callback_credential_schema": "fileAesKey+fileId+fileMd5+fileSize+filename",
        "callback_additional_field_count": 0,
        "external_send_executed": True,
        "safe_for_chat": False,
        "limitations": ["fixed limitation"],
        "guardrails": ["fixed guardrail"],
    }
    report.update(overrides)
    return report


class QiWeImageCallbackBridgeTests(unittest.TestCase):
    def make_processor(
        self,
        directory: Path,
        output: str,
        *,
        delay: float = 0.0,
        check_environment: bool = False,
    ) -> Path:
        directory.mkdir(parents=True, exist_ok=True)
        directory.chmod(0o700)
        path = directory / ("b" * 40) / "sidecar" / "qintopia-message-sidecar"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            f"#!{sys.executable}\n"
            "import os, sys, time\n"
            f"time.sleep({delay!r})\n"
            "body = sys.stdin.buffer.read()\n"
            "expected = ['process-qiwe-image-send-callback', '--apply']\n"
            "if sys.argv[1:] != expected or b'raw-aes-secret' not in body:\n"
            "    raise SystemExit(7)\n"
            f"if {check_environment!r}:\n"
            "    if os.environ.get('QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL') != 'approved-staging-qiwe-image-send':\n"
            "        raise SystemExit(8)\n"
            "    if 'QINTOPIA_UNRELATED_RUNTIME_SECRET' in os.environ:\n"
            "        raise SystemExit(9)\n"
            f"sys.stdout.write({output!r})\n",
            encoding="utf-8",
        )
        path.chmod(0o700)
        return path.resolve()

    def make_production_processor(
        self,
        releases_root: Path,
        output: str,
        *,
        check_environment: bool = False,
    ) -> Path:
        releases_root.mkdir(parents=True, exist_ok=True)
        releases_root.chmod(0o700)
        release_dir = releases_root / ("c" * 40)
        path = release_dir / "sidecar" / "qintopia-message-sidecar"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            f"#!{sys.executable}\n"
            "import os, sys\n"
            "body = sys.stdin.buffer.read()\n"
            "expected = ['process-qiwe-image-send-callback', '--apply']\n"
            "if sys.argv[1:] != expected or b'raw-aes-secret' not in body:\n"
            "    raise SystemExit(7)\n"
            f"if {check_environment!r}:\n"
            "    if os.environ.get('QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL') != 'approved-production-qiwe-image-send':\n"
            "        raise SystemExit(8)\n"
            "    if os.environ.get('QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN') != 'bascnReviewed':\n"
            "        raise SystemExit(9)\n"
            "    if 'QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL' in os.environ:\n"
            "        raise SystemExit(10)\n"
            "    if 'QINTOPIA_UNRELATED_RUNTIME_SECRET' in os.environ:\n"
            "        raise SystemExit(11)\n"
            f"sys.stdout.write({output!r})\n",
            encoding="utf-8",
        )
        path.chmod(0o700)
        current = releases_root / "current"
        current.symlink_to(release_dir.name)
        return current / "sidecar" / "qintopia-message-sidecar"

    def bridge(self, processor: Path) -> QiWeImageCallbackBridge:
        return QiWeImageCallbackBridge(
            enabled=True,
            processor_bin=str(processor),
            processor_root=str(processor.parent.parent),
            processor_sha256=hashlib.sha256(processor.read_bytes()).hexdigest(),
            timeout_seconds=5.0,
            staging_approval="approved-staging-qiwe-image-send",
            staging_database_url_sha256="a" * 64,
            image_send_enabled="1",
            webhook_ready="1",
        )

    def production_bridge(self, processor: Path) -> QiWeImageCallbackBridge:
        return QiWeImageCallbackBridge(
            enabled=True,
            processor_bin=str(processor),
            processor_root=str(processor.parent.parent),
            processor_sha256=hashlib.sha256(processor.read_bytes()).hexdigest(),
            processor_mode="production",
            timeout_seconds=5.0,
            production_approval="approved-production-qiwe-image-send",
            production_database_url_sha256="c" * 64,
            image_send_enabled="1",
            webhook_ready="1",
        )

    def test_detects_reviewed_callback_envelope_without_exposing_values(self) -> None:
        self.assertTrue(is_async_image_callback(callback_body()))
        self.assertFalse(
            is_async_image_callback(b'{"outer":{"CMD":"20000","secret":"value"}}')
        )
        self.assertFalse(is_async_image_callback(b'{"code":0,"data":[{"cmd":20000}]}'))
        self.assertFalse(is_async_image_callback(b'{"cmd":15000}'))
        self.assertFalse(is_async_image_callback(b"not-json"))

    def test_deep_or_oversized_callback_structure_fails_closed(self) -> None:
        deeply_nested = (
            b"[" * 2_000 + b'{"cmd":20000}' + b"]" * 2_000
        )
        self.assertFalse(is_async_image_callback(deeply_nested))

        too_many_events = json.dumps({"code": 0, "data": [None] * 65}).encode()
        self.assertFalse(is_async_image_callback(too_many_events))

    def test_disabled_bridge_never_requires_or_invokes_processor(self) -> None:
        result = asyncio.run(
            QiWeImageCallbackBridge(enabled=False).process(callback_body())
        )

        self.assertEqual(
            result,
            CallbackBridgeResult(True, False, False, "processor_disabled"),
        )

    def test_enabled_bridge_requires_staging_gates_and_executable(self) -> None:
        with self.assertRaises(ValueError):
            QiWeImageCallbackBridge(
                enabled=True,
                processor_bin="/production/qintopia-message-sidecar",
                staging_approval="approved-staging-qiwe-image-send",
                staging_database_url_sha256="a" * 64,
                image_send_enabled="1",
                webhook_ready="1",
            )
        with self.assertRaises(ValueError):
            QiWeImageCallbackBridge(
                enabled=True,
                processor_bin="/tmp/staging/qintopia-message-sidecar",
                staging_approval="wrong",
                staging_database_url_sha256="a" * 64,
                image_send_enabled="1",
                webhook_ready="1",
            )
        with self.assertRaises(ValueError):
            QiWeImageCallbackBridge(
                enabled=True,
                processor_bin="/tmp/staging/qintopia-message-sidecar",
                staging_approval="approved-staging-qiwe-image-send",
                staging_database_url_sha256="A" * 64,
                image_send_enabled="1",
                webhook_ready="1",
            )

    def test_callback_streams_to_fixed_command_and_returns_sanitized_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            releases_root = Path(tmp)
            with mock.patch.object(
                image_callback_bridge, "STAGING_RELEASES_ROOT", releases_root
            ):
                processor = self.make_processor(
                    releases_root,
                    json.dumps(callback_report(), separators=(",", ":")),
                )
                result = asyncio.run(self.bridge(processor).process(callback_body()))

        self.assertTrue(result.processed)
        self.assertEqual(result.action_status, "image_send_completed")
        self.assertEqual(
            result.callback_credential_schema,
            "fileAesKey+fileId+fileMd5+fileSize+filename",
        )
        self.assertEqual(result.callback_additional_field_count, 0)
        self.assertIs(result.external_send_executed, True)
        serialized = repr(result)
        for secret in (
            "raw-request-secret",
            "raw-aes-secret",
            "raw-file-secret",
            "private-activity-poster.jpg",
        ):
            self.assertNotIn(secret, serialized)

    def test_processor_environment_is_exactly_allowlisted(self) -> None:
        self.assertEqual(
            set(PROCESSOR_ENV_ALLOWLIST),
            {
                "QINTOPIA_SIDECAR_DATABASE_URL",
                "QINTOPIA_SIDECAR_DB_MAX_CONNECTIONS",
                "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256",
                "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
                "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
                "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
                "QIWE_API_URL",
                "QIWE_TOKEN",
                "QIWE_GUID",
                "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
                "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
                "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
            },
        )
        names = [
            "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
            "QINTOPIA_UNRELATED_RUNTIME_SECRET",
        ]
        original = {name: os.environ.get(name) for name in names}
        try:
            os.environ["QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL"] = (
                "approved-staging-qiwe-image-send"
            )
            os.environ["QINTOPIA_UNRELATED_RUNTIME_SECRET"] = "must-not-reach-child"
            with tempfile.TemporaryDirectory() as tmp:
                releases_root = Path(tmp)
                with mock.patch.object(
                    image_callback_bridge, "STAGING_RELEASES_ROOT", releases_root
                ):
                    processor = self.make_processor(
                        releases_root,
                        json.dumps(callback_report(), separators=(",", ":")),
                        check_environment=True,
                    )
                    result = asyncio.run(
                        self.bridge(processor).process(callback_body())
                    )
        finally:
            for name, value in original.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value

        self.assertTrue(result.processed)

    def test_production_processor_environment_is_exactly_allowlisted(self) -> None:
        self.assertEqual(
            set(PRODUCTION_PROCESSOR_ENV_ALLOWLIST),
            {
                "QINTOPIA_SIDECAR_DATABASE_URL",
                "QINTOPIA_SIDECAR_DB_MAX_CONNECTIONS",
                "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
                "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
                "QIWE_API_URL",
                "QIWE_TOKEN",
                "QIWE_GUID",
                "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
                "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
                "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
                "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
                "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
                "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
                "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
                "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA",
                "QINTOPIA_DEPLOYED_COMMIT_SHA",
                "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
                "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
                "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
                "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
                "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
                "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH",
                "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
                "QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES",
            },
        )
        names = [
            "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
            "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
            "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
            "QINTOPIA_UNRELATED_RUNTIME_SECRET",
        ]
        original = {name: os.environ.get(name) for name in names}
        try:
            os.environ["QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL"] = (
                "approved-production-qiwe-image-send"
            )
            os.environ["QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN"] = "bascnReviewed"
            os.environ["QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL"] = (
                "approved-staging-qiwe-image-send"
            )
            os.environ["QINTOPIA_UNRELATED_RUNTIME_SECRET"] = "must-not-reach-child"
            with tempfile.TemporaryDirectory() as tmp:
                releases_root = Path(tmp)
                with (
                    mock.patch.object(
                        image_callback_bridge, "PRODUCTION_RELEASES_ROOT", releases_root
                    ),
                    mock.patch.object(
                        image_callback_bridge,
                        "PRODUCTION_RELEASE_CURRENT_DIR",
                        releases_root / "current",
                    ),
                ):
                    processor = self.make_production_processor(
                        releases_root,
                        json.dumps(callback_report(), separators=(",", ":")),
                        check_environment=True,
                    )
                    result = asyncio.run(
                        self.production_bridge(processor).process(callback_body())
                    )
        finally:
            for name, value in original.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value

        self.assertTrue(result.processed)

    def test_unknown_report_fields_and_oversized_stdout_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            releases_root = Path(tmp)
            with mock.patch.object(
                image_callback_bridge, "STAGING_RELEASES_ROOT", releases_root
            ):
                report = callback_report(raw_callback_secret="must-not-escape")
                processor = self.make_processor(releases_root, json.dumps(report))
                invalid = asyncio.run(self.bridge(processor).process(callback_body()))
                processor = self.make_processor(
                    releases_root, json.dumps(callback_report(success=False))
                )
                inconsistent = asyncio.run(
                    self.bridge(processor).process(callback_body())
                )
                processor.write_text(
                    "#!/usr/bin/env python3\n"
                    "import sys\n"
                    "sys.stdin.buffer.read()\n"
                    f"sys.stdout.write('x' * {MAX_REPORT_BYTES + 1})\n",
                    encoding="utf-8",
                )
                processor.chmod(0o700)
                oversized = asyncio.run(
                    self.bridge(processor).process(callback_body())
                )

        self.assertEqual(invalid.reason, "processor_failed")
        self.assertFalse(invalid.processed)
        self.assertEqual(inconsistent.reason, "processor_failed")
        self.assertFalse(inconsistent.processed)
        self.assertEqual(oversized.reason, "processor_failed")
        self.assertFalse(oversized.processed)

    def test_timeout_terminates_processor_without_returning_callback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            releases_root = Path(tmp)
            with mock.patch.object(
                image_callback_bridge, "STAGING_RELEASES_ROOT", releases_root
            ):
                processor = self.make_processor(
                    releases_root, json.dumps(callback_report()), delay=1.0
                )
                bridge = self.bridge(processor)
                bridge.timeout_seconds = 0.01
                result = asyncio.run(bridge.process(callback_body()))

        self.assertEqual(result.reason, "processor_timeout")
        self.assertFalse(result.processed)
        self.assertNotIn("raw-aes-secret", repr(result))

    def test_processor_requires_fixed_release_root_digest_and_safe_modes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            container = Path(tmp)
            releases_root = container / "approved-staging-releases"
            with mock.patch.object(
                image_callback_bridge, "STAGING_RELEASES_ROOT", releases_root
            ):
                processor = self.make_processor(
                    releases_root, json.dumps(callback_report())
                )
                digest = hashlib.sha256(processor.read_bytes()).hexdigest()

                self.bridge(processor)
                with self.assertRaises(ValueError):
                    QiWeImageCallbackBridge(
                        enabled=True,
                        processor_bin=str(processor),
                        processor_root=str(processor.parent.parent),
                        processor_sha256="c" * 64,
                        staging_approval="approved-staging-qiwe-image-send",
                        staging_database_url_sha256="a" * 64,
                        image_send_enabled="1",
                        webhook_ready="1",
                    )

                processor.parent.chmod(0o770)
                with self.assertRaises(ValueError):
                    QiWeImageCallbackBridge(
                        enabled=True,
                        processor_bin=str(processor),
                        processor_root=str(processor.parent.parent),
                        processor_sha256=digest,
                        staging_approval="approved-staging-qiwe-image-send",
                        staging_database_url_sha256="a" * 64,
                        image_send_enabled="1",
                        webhook_ready="1",
                    )
                processor.parent.chmod(0o755)

                outside = self.make_processor(
                    container / "writable-staging", json.dumps(callback_report())
                )
                with self.assertRaises(ValueError):
                    self.bridge(outside)

    def test_production_processor_requires_release_current_digest_and_safe_modes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            releases_root = Path(tmp)
            with (
                mock.patch.object(
                    image_callback_bridge, "PRODUCTION_RELEASES_ROOT", releases_root
                ),
                mock.patch.object(
                    image_callback_bridge,
                    "PRODUCTION_RELEASE_CURRENT_DIR",
                    releases_root / "current",
                ),
            ):
                processor = self.make_production_processor(
                    releases_root, json.dumps(callback_report())
                )
                digest = hashlib.sha256(processor.read_bytes()).hexdigest()

                self.production_bridge(processor)
                with self.assertRaises(ValueError):
                    QiWeImageCallbackBridge(
                        enabled=True,
                        processor_bin=str(processor),
                        processor_root=str(processor.parent.parent),
                        processor_sha256=digest,
                        processor_mode="production",
                        staging_approval="approved-staging-qiwe-image-send",
                        staging_database_url_sha256="a" * 64,
                        image_send_enabled="1",
                        webhook_ready="1",
                    )
                with self.assertRaises(ValueError):
                    QiWeImageCallbackBridge(
                        enabled=True,
                        processor_bin=str(processor),
                        processor_root=str(processor.parent.parent),
                        processor_sha256="d" * 64,
                        processor_mode="production",
                        production_approval="approved-production-qiwe-image-send",
                        production_database_url_sha256="c" * 64,
                        image_send_enabled="1",
                        webhook_ready="1",
                    )

                processor.parent.chmod(0o770)
                with self.assertRaises(ValueError):
                    self.production_bridge(processor)
                processor.parent.chmod(0o755)

                outside = self.make_processor(
                    releases_root / "staging-like", json.dumps(callback_report())
                )
                with self.assertRaises(ValueError):
                    QiWeImageCallbackBridge(
                        enabled=True,
                        processor_bin=str(outside),
                        processor_root=str(outside.parent.parent),
                        processor_sha256=hashlib.sha256(outside.read_bytes()).hexdigest(),
                        processor_mode="production",
                        production_approval="approved-production-qiwe-image-send",
                        production_database_url_sha256="c" * 64,
                        image_send_enabled="1",
                        webhook_ready="1",
                    )

                direct_release_path = processor.resolve()
                with self.assertRaises(ValueError):
                    QiWeImageCallbackBridge(
                        enabled=True,
                        processor_bin=str(direct_release_path),
                        processor_root=str(direct_release_path.parent.parent),
                        processor_sha256=digest,
                        processor_mode="production",
                        production_approval="approved-production-qiwe-image-send",
                        production_database_url_sha256="c" * 64,
                        image_send_enabled="1",
                        webhook_ready="1",
                    )

    def test_processor_digest_is_rechecked_immediately_before_spawn(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            releases_root = Path(tmp)
            with mock.patch.object(
                image_callback_bridge, "STAGING_RELEASES_ROOT", releases_root
            ):
                processor = self.make_processor(
                    releases_root, json.dumps(callback_report())
                )
                bridge = self.bridge(processor)
                processor.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                processor.chmod(0o700)
                result = asyncio.run(bridge.process(callback_body()))

        self.assertEqual(result.reason, "processor_failed")
        self.assertFalse(result.processed)

    def test_environment_enablement_is_exact_and_defaults_disabled(self) -> None:
        names = [
            "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED",
            "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE",
            "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN",
            "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT",
            "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_SHA256",
            "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_TIMEOUT_SECONDS",
            "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
            "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256",
            "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
            "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
            "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
            "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
        ]
        original = {name: os.environ.get(name) for name in names}
        try:
            for name in names:
                os.environ.pop(name, None)
            self.assertFalse(QiWeImageCallbackBridge.from_environment().enabled)
            os.environ["QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED"] = "1"
            invalid = QiWeImageCallbackBridge.from_environment()
            self.assertTrue(invalid.enabled)
            self.assertFalse(invalid.configuration_valid)
            result = asyncio.run(invalid.process(callback_body()))
            self.assertEqual(result.reason, "processor_configuration_invalid")
            self.assertFalse(result.processed)
            os.environ["QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED"] = "true"
            invalid_flag = QiWeImageCallbackBridge.from_environment()
            self.assertTrue(invalid_flag.enabled)
            self.assertFalse(invalid_flag.configuration_valid)
            result = asyncio.run(invalid_flag.process(callback_body()))
            self.assertEqual(result.reason, "processor_configuration_invalid")
        finally:
            for name, value in original.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value
