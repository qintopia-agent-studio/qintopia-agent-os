from __future__ import annotations

import asyncio
import hashlib
import json
import os
import stat
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping


ASYNC_CALLBACK_COMMAND = 20_000
MAX_CALLBACK_BYTES = 64 * 1024
MAX_REPORT_BYTES = 16 * 1024
MAX_CALLBACK_EVENTS = 64
MAX_PROCESSOR_BYTES = 128 * 1024 * 1024
PROCESSOR_BASENAME = "qintopia-message-sidecar"
PROCESSOR_ARGS = ("process-qiwe-image-send-callback", "--apply")
PROCESSOR_MODE_STAGING = "staging"
PROCESSOR_MODE_PRODUCTION = "production"
STAGING_RELEASES_ROOT = Path("/home/ubuntu/qintopia-agent-os-staging-releases")
PRODUCTION_RELEASES_ROOT = Path("/home/ubuntu/qintopia-agent-os-releases")
PRODUCTION_RELEASE_CURRENT_DIR = PRODUCTION_RELEASES_ROOT / "current"
STAGING_APPROVAL = "approved-staging-qiwe-image-send"
PRODUCTION_APPROVAL = "approved-production-qiwe-image-send"
COMMON_PROCESSOR_ENV_ALLOWLIST = (
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
)
PROCESSOR_ENV_ALLOWLIST = (
    *COMMON_PROCESSOR_ENV_ALLOWLIST,
    "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
)
PRODUCTION_PROCESSOR_ENV_ALLOWLIST = (
    *COMMON_PROCESSOR_ENV_ALLOWLIST,
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
)
CALLBACK_SCHEMAS = {
    "fileAesKey+fileId+fileMd5+fileSize+filename",
    "fileAeskey+fileId+fileMd5+fileSize+filename",
    "fileAesKey+fileId+fileMd5+fileSize+fileName",
    "fileAeskey+fileId+fileMd5+fileSize+fileName",
}
ACTION_BOUNDARIES = {
    "callback_expired": (True, False, False),
    "send_request_rejected": (False, False, True),
    "image_send_completed": (True, True, True),
    "image_send_not_sent": (False, False, True),
    "image_send_ambiguous": (False, None, True),
    "callback_duplicate_sending": (True, None, False),
    "callback_duplicate_sent": (True, None, False),
    "callback_duplicate_failed": (True, None, False),
    "callback_duplicate_ambiguous": (True, None, False),
    "callback_duplicate_expired": (True, None, False),
}
REPORT_KEYS = {
    "success",
    "dry_run",
    "apply_requested",
    "worker",
    "phase",
    "action_status",
    "work_item_id",
    "external_upload_requested",
    "callback_received",
    "callback_credential_schema",
    "callback_additional_field_count",
    "external_send_executed",
    "safe_for_chat",
    "limitations",
    "guardrails",
}


@dataclass(frozen=True)
class CallbackBridgeResult:
    detected: bool
    enabled: bool
    processed: bool
    reason: str
    action_status: str | None = None
    callback_credential_schema: str | None = None
    callback_additional_field_count: int | None = None
    external_send_executed: bool | None = None


class QiWeImageCallbackBridge:
    def __init__(
        self,
        *,
        enabled: bool,
        processor_bin: str = "",
        processor_root: str = "",
        processor_sha256: str = "",
        processor_mode: str = PROCESSOR_MODE_STAGING,
        timeout_seconds: float = 30.0,
        staging_approval: str = "",
        staging_database_url_sha256: str = "",
        production_approval: str = "",
        production_database_url_sha256: str = "",
        image_send_enabled: str = "0",
        webhook_ready: str = "0",
        configuration_valid: bool = True,
    ) -> None:
        self.enabled = enabled
        self.configuration_valid = configuration_valid
        self.timeout_seconds = timeout_seconds
        self.processor_mode = processor_mode
        self.processor_bin = ""
        self.processor_root = ""
        self.processor_sha256 = ""
        if processor_mode == PROCESSOR_MODE_PRODUCTION:
            env_allowlist = PRODUCTION_PROCESSOR_ENV_ALLOWLIST
        else:
            env_allowlist = PROCESSOR_ENV_ALLOWLIST
        self.processor_env = {
            name: os.environ[name]
            for name in env_allowlist
            if name in os.environ
        }
        if not enabled or not configuration_valid:
            return
        if processor_mode == PROCESSOR_MODE_STAGING:
            if staging_approval != STAGING_APPROVAL:
                raise ValueError("staging owner approval is required")
            if not _is_canonical_sha256(staging_database_url_sha256):
                raise ValueError("approved staging database URL hash is required")
        elif processor_mode == PROCESSOR_MODE_PRODUCTION:
            if production_approval != PRODUCTION_APPROVAL:
                raise ValueError("production owner approval is required")
            if not _is_canonical_sha256(production_database_url_sha256):
                raise ValueError("approved production database URL hash is required")
        else:
            raise ValueError("callback processor mode is invalid")
        if image_send_enabled != "1" or webhook_ready != "1":
            raise ValueError("image send and webhook readiness are required")
        if not 1.0 <= timeout_seconds <= 60.0:
            raise ValueError("callback processor timeout is outside the reviewed range")
        self.processor_bin, self.processor_root = _validated_processor_path(
            processor_bin, processor_root, processor_sha256, processor_mode
        )
        self.processor_sha256 = processor_sha256

    @classmethod
    def from_environment(
        cls, extra: Mapping[str, Any] | None = None
    ) -> "QiWeImageCallbackBridge":
        extra = extra or {}
        try:
            enabled = _strict_enabled(
                os.getenv("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED"),
                extra.get("image_callback_processor_enabled"),
            )
        except ValueError:
            return cls(enabled=True, configuration_valid=False)
        try:
            timeout_raw = os.getenv(
                "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_TIMEOUT_SECONDS"
            )
            if timeout_raw is None:
                timeout_raw = extra.get(
                    "image_callback_processor_timeout_seconds", 30.0
                )
            try:
                timeout_seconds = float(timeout_raw)
            except (TypeError, ValueError) as exc:
                raise ValueError("callback processor timeout must be numeric") from exc
            processor_mode = str(
                os.getenv("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE")
                or extra.get(
                    "image_callback_processor_mode", PROCESSOR_MODE_STAGING
                )
            )
            return cls(
                enabled=enabled,
                processor_bin=str(
                    os.getenv("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN")
                    or extra.get("image_callback_processor_bin", "")
                ),
                processor_root=os.getenv(
                    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT"
                )
                or str(extra.get("image_callback_processor_root", "")),
                processor_sha256=os.getenv(
                    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_SHA256"
                )
                or str(extra.get("image_callback_processor_sha256", "")),
                processor_mode=processor_mode,
                timeout_seconds=timeout_seconds,
                staging_approval=os.getenv(
                    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL", ""
                ),
                staging_database_url_sha256=os.getenv(
                    "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256", ""
                ),
                production_approval=os.getenv(
                    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL", ""
                ),
                production_database_url_sha256=os.getenv(
                    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256", ""
                ),
                image_send_enabled=os.getenv("QINTOPIA_QIWE_IMAGE_SEND_ENABLED", "0"),
                webhook_ready=os.getenv(
                    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY", "0"
                ),
            )
        except ValueError:
            if enabled:
                return cls(enabled=True, configuration_valid=False)
            raise

    async def process(self, raw_body: bytes) -> CallbackBridgeResult:
        if not is_async_image_callback(raw_body):
            return CallbackBridgeResult(False, self.enabled, False, "not_async_callback")
        if not self.enabled:
            return CallbackBridgeResult(True, False, False, "processor_disabled")
        if not self.configuration_valid:
            return CallbackBridgeResult(
                True, True, False, "processor_configuration_invalid"
            )
        if not raw_body or len(raw_body) > MAX_CALLBACK_BYTES:
            return CallbackBridgeResult(True, True, False, "callback_size_invalid")
        try:
            report = await asyncio.wait_for(
                self._invoke(raw_body), timeout=self.timeout_seconds
            )
        except asyncio.TimeoutError:
            return CallbackBridgeResult(True, True, False, "processor_timeout")
        except (OSError, ValueError, json.JSONDecodeError):
            return CallbackBridgeResult(True, True, False, "processor_failed")
        return CallbackBridgeResult(
            detected=True,
            enabled=True,
            processed=True,
            reason="processor_completed",
            action_status=report["action_status"],
            callback_credential_schema=report["callback_credential_schema"],
            callback_additional_field_count=report[
                "callback_additional_field_count"
            ],
            external_send_executed=report["external_send_executed"],
        )

    async def _invoke(self, raw_body: bytes) -> dict[str, Any]:
        _validated_processor_path(
            self.processor_bin,
            self.processor_root,
            self.processor_sha256,
            self.processor_mode,
        )
        process = await asyncio.create_subprocess_exec(
            self.processor_bin,
            *PROCESSOR_ARGS,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL,
            env=self.processor_env,
        )
        try:
            stdout = await _exchange_bounded(process, raw_body)
        except BaseException:
            if process.returncode is None:
                process.kill()
                await process.wait()
            raise
        if process.returncode != 0:
            raise ValueError("callback processor exited unsuccessfully")
        report = json.loads(stdout)
        _validate_report(report)
        return report


async def _exchange_bounded(
    process: asyncio.subprocess.Process, raw_body: bytes
) -> bytes:
    if process.stdin is None or process.stdout is None:
        raise ValueError("callback processor pipes are unavailable")

    async def write_input() -> None:
        process.stdin.write(raw_body)
        await process.stdin.drain()
        process.stdin.close()

    async def read_output() -> bytes:
        output = bytearray()
        while True:
            chunk = await process.stdout.read(4096)
            if not chunk:
                return bytes(output)
            output.extend(chunk)
            if len(output) > MAX_REPORT_BYTES:
                raise ValueError("callback processor report is too large")

    writer = asyncio.create_task(write_input())
    reader = asyncio.create_task(read_output())
    try:
        await writer
        output = await reader
        await process.wait()
        return output
    finally:
        for task in (writer, reader):
            if not task.done():
                task.cancel()


def is_async_image_callback(raw_body: bytes) -> bool:
    try:
        value = json.loads(raw_body)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError):
        return False
    if not isinstance(value, dict):
        return False
    code = _case_insensitive_value(value, "code")
    events = _case_insensitive_value(value, "data")
    if type(code) is not int or code != 0 or not isinstance(events, list):
        return False
    if not events or len(events) > MAX_CALLBACK_EVENTS:
        return False
    return any(_is_async_image_callback_event(event) for event in events)


def _is_async_image_callback_event(value: Any) -> bool:
    if not isinstance(value, dict):
        return False
    command = _case_insensitive_value(value, "cmd")
    if isinstance(command, bool) or str(command).strip() != str(ASYNC_CALLBACK_COMMAND):
        return False
    request_id = _case_insensitive_value(value, "requestid")
    msg_data = _case_insensitive_value(value, "msgdata")
    if not isinstance(request_id, str) or not request_id.strip():
        return False
    if not isinstance(msg_data, dict):
        return False
    fields = {str(key).lower() for key in msg_data}
    return {
        "fileaeskey",
        "fileid",
        "filemd5",
        "filesize",
        "filename",
    }.issubset(fields)


def _case_insensitive_value(value: dict[Any, Any], expected: str) -> Any:
    return next(
        (item for key, item in value.items() if str(key).lower() == expected),
        None,
    )


def _strict_enabled(env_value: str | None, extra_value: Any) -> bool:
    if env_value is not None:
        if env_value not in {"0", "1"}:
            raise ValueError("callback processor enable flag must be 0 or 1")
        return env_value == "1"
    if extra_value in (None, False, 0, "0"):
        return False
    if extra_value in (True, 1, "1"):
        return True
    raise ValueError("callback processor enable flag must be explicit")


def _is_canonical_sha256(value: str) -> bool:
    return len(value) == 64 and all(char in "0123456789abcdef" for char in value)


def _validated_processor_path(
    value: str,
    root_value: str,
    expected_sha256: str,
    processor_mode: str = PROCESSOR_MODE_STAGING,
) -> tuple[str, str]:
    candidate = Path(value)
    root = Path(root_value)
    if (
        not candidate.is_absolute()
        or candidate.name != PROCESSOR_BASENAME
        or not root.is_absolute()
    ):
        raise ValueError("callback processor path is invalid")
    if not _is_canonical_sha256(expected_sha256):
        raise ValueError("callback processor SHA-256 is invalid")
    if processor_mode == PROCESSOR_MODE_PRODUCTION:
        return _validated_production_processor_path(candidate, root, expected_sha256)
    if processor_mode != PROCESSOR_MODE_STAGING:
        raise ValueError("callback processor mode is invalid")
    if len(root.name) != 40 or any(char not in "0123456789abcdef" for char in root.name):
        raise ValueError("callback processor path is invalid")
    try:
        resolved_releases_root = STAGING_RELEASES_ROOT.resolve(strict=True)
        resolved = candidate.resolve(strict=True)
        resolved_root = root.resolve(strict=True)
        resolved_sidecar = candidate.parent.resolve(strict=True)
    except OSError as exc:
        raise ValueError("callback processor path does not exist") from exc
    if (
        candidate.is_symlink()
        or candidate.parent.is_symlink()
        or root.is_symlink()
        or STAGING_RELEASES_ROOT.is_symlink()
        or not resolved.is_file()
        or not resolved_releases_root.is_dir()
        or not resolved_root.is_dir()
        or resolved_root.parent != resolved_releases_root
        or resolved_sidecar.name != "sidecar"
        or resolved_sidecar.parent != resolved_root
        or resolved.parent != resolved_sidecar
        or not os.access(resolved, os.X_OK)
    ):
        raise ValueError("callback processor must be a regular executable")
    _validate_processor_path_chain(
        resolved_releases_root, resolved_root, resolved_sidecar, resolved
    )
    _validate_processor_digest(resolved, expected_sha256)
    return str(resolved), str(resolved_root)


def _validated_production_processor_path(
    candidate: Path, root: Path, expected_sha256: str
) -> tuple[str, str]:
    if not candidate.is_absolute() or candidate.name != PROCESSOR_BASENAME:
        raise ValueError("callback processor path is invalid")
    if not root.is_absolute():
        raise ValueError("callback processor path is invalid")
    try:
        resolved_releases_root = PRODUCTION_RELEASES_ROOT.resolve(strict=True)
        resolved_current = PRODUCTION_RELEASE_CURRENT_DIR.resolve(strict=True)
        resolved = candidate.resolve(strict=True)
        resolved_root = root.resolve(strict=True)
        resolved_sidecar = candidate.parent.resolve(strict=True)
    except OSError as exc:
        raise ValueError("callback processor path does not exist") from exc
    if (
        root != PRODUCTION_RELEASE_CURRENT_DIR
        or candidate
        != PRODUCTION_RELEASE_CURRENT_DIR / "sidecar" / PROCESSOR_BASENAME
        or not PRODUCTION_RELEASE_CURRENT_DIR.is_symlink()
        or len(resolved_current.name) != 40
        or any(char not in "0123456789abcdef" for char in resolved_current.name)
        or resolved_current.parent != resolved_releases_root
        or resolved_root != resolved_current
        or resolved_sidecar.name != "sidecar"
        or resolved_sidecar.parent != resolved_current
        or resolved.parent != resolved_sidecar
        or resolved
        != resolved_current / "sidecar" / PROCESSOR_BASENAME
        or candidate.is_symlink()
        or candidate.parent.is_symlink()
        or PRODUCTION_RELEASES_ROOT.is_symlink()
        or not resolved.is_file()
        or not os.access(resolved, os.X_OK)
    ):
        raise ValueError("callback processor must be the production release/current executable")
    _validate_processor_path_chain(
        resolved_releases_root, resolved_current, resolved_sidecar, resolved
    )
    _validate_processor_digest(resolved, expected_sha256)
    return str(candidate), str(root)


def _validate_processor_path_chain(*paths: Path) -> None:
    allowed_owners = {0, os.geteuid()}
    for path in paths:
        path_stat = path.stat()
        if path_stat.st_uid not in allowed_owners:
            raise ValueError("callback processor path owner is invalid")
        if stat.S_IMODE(path_stat.st_mode) & 0o022:
            raise ValueError("callback processor path is group or world writable")


def _validate_processor_digest(processor: Path, expected_sha256: str) -> None:
    size = processor.stat().st_size
    if not 0 < size <= MAX_PROCESSOR_BYTES:
        raise ValueError("callback processor size is invalid")
    digest = hashlib.sha256()
    with processor.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    if digest.hexdigest() != expected_sha256:
        raise ValueError("callback processor SHA-256 does not match")


def _validate_report(report: Any) -> None:
    if not isinstance(report, dict) or set(report) != REPORT_KEYS:
        raise ValueError("callback processor report shape is invalid")
    if (
        report["worker"] != "qiwe-image-send-adapter"
        or report["phase"] != "callback"
        or report["dry_run"] is not False
        or report["apply_requested"] is not True
        or report["external_upload_requested"] is not False
        or report["callback_received"] is not True
        or report["safe_for_chat"] is not False
    ):
        raise ValueError("callback processor report boundary is invalid")
    action_status = report["action_status"]
    if not isinstance(action_status, str) or action_status not in ACTION_BOUNDARIES:
        raise ValueError("callback processor action is not allowlisted")
    schema = report["callback_credential_schema"]
    if schema not in CALLBACK_SCHEMAS:
        raise ValueError("callback credential schema is not allowlisted")
    additional_fields = report["callback_additional_field_count"]
    if (
        isinstance(additional_fields, bool)
        or not isinstance(additional_fields, int)
        or not 0 <= additional_fields <= 64
    ):
        raise ValueError("callback additional-field count is invalid")
    external_send_executed = report["external_send_executed"]
    if external_send_executed is not None and not isinstance(
        external_send_executed, bool
    ):
        raise ValueError("callback external-send result is invalid")
    work_item_id = report["work_item_id"]
    if work_item_id is not None:
        uuid.UUID(str(work_item_id))
    success = report["success"]
    if not isinstance(success, bool):
        raise ValueError("callback success state is invalid")
    expected_success, expected_send, work_item_required = ACTION_BOUNDARIES[
        action_status
    ]
    if (
        success is not expected_success
        or external_send_executed is not expected_send
        or (work_item_id is not None) is not work_item_required
    ):
        raise ValueError("callback processor outcome is inconsistent")
    if not all(
        isinstance(report[field], list)
        and all(isinstance(item, str) for item in report[field])
        for field in ("limitations", "guardrails")
    ):
        raise ValueError("callback report guardrails are invalid")
