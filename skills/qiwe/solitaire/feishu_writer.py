from __future__ import annotations

import json
import logging
import os
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from datetime import date, datetime, time, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional
from zoneinfo import ZoneInfo

logger = logging.getLogger(__name__)
_SKIP_FIELD = object()

KNOWN_INTERNAL_FIELDS = {
    "activity_id",
    "source_group_id",
    "source_message_id",
    "source_sender_id",
    "activity_subject",
    "activity_identity",
    "stable_body_fingerprint",
    "activity_type",
    "activity_detail",
    "start_time",
    "solitaire_created_at",
    "participant_names",
    "participant_count",
    "promo_text",
    "status",
    "raw_summary",
    "last_seen_at",
}


@dataclass
class FeishuActivityMapping:
    enabled: bool = False
    provider: str = "feishu_bitable"
    app_token_env: str = "QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN"
    table_id_env: str = "QINTOPIA_FEISHU_ACTIVITY_TABLE_ID"
    record_id_state_key: str = "activity_id"
    upsert_key: str = "activity_id"
    mode: str = "dry_run"
    fields: Dict[str, str] = field(default_factory=dict)
    field_types: Dict[str, str] = field(default_factory=dict)
    default_fields: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, payload: Dict[str, Any]) -> "FeishuActivityMapping":
        fields = payload.get("fields") if isinstance(payload.get("fields"), dict) else {}
        field_types = payload.get("fieldTypes") if isinstance(payload.get("fieldTypes"), dict) else {}
        default_fields = payload.get("defaultFields") if isinstance(payload.get("defaultFields"), dict) else {}
        return cls(
            enabled=bool(payload.get("enabled", False)),
            provider=str(payload.get("provider") or "feishu_bitable"),
            app_token_env=str(payload.get("appTokenEnv") or "QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN"),
            table_id_env=str(payload.get("tableIdEnv") or "QINTOPIA_FEISHU_ACTIVITY_TABLE_ID"),
            record_id_state_key=str(payload.get("recordIdStateKey") or "activity_id"),
            upsert_key=str(payload.get("upsertKey") or "activity_id"),
            mode=str(payload.get("mode") or "dry_run"),
            fields={str(key): str(value) for key, value in fields.items()},
            field_types={str(key): str(value) for key, value in field_types.items()},
            default_fields={str(key): value for key, value in default_fields.items()},
        )

    @classmethod
    def load(cls, path: str) -> "FeishuActivityMapping":
        if not path:
            return cls()
        payload = json.loads(Path(path).expanduser().read_text(encoding="utf-8"))
        if not isinstance(payload, dict):
            raise ValueError("Feishu activity mapping must be a JSON object")
        if isinstance(payload.get("sinks"), list) and payload["sinks"]:
            first = dict(payload["sinks"][0])
            for key in ("enabled", "provider", "mode"):
                if key in payload and key not in first:
                    first[key] = payload[key]
            return cls.from_dict(first)
        return cls.from_dict(payload)

    def validate(self) -> list[str]:
        errors = []
        if self.provider != "feishu_bitable":
            errors.append(f"unsupported provider: {self.provider}")
        if self.upsert_key not in self.fields or not self.fields.get(self.upsert_key):
            errors.append("upsertKey must be mapped to a non-empty Feishu field")
        if self.mode not in {"dry_run", "live"}:
            errors.append("mode must be dry_run or live")
        return errors


@dataclass
class FeishuWriteResult:
    success: bool
    mode: str
    mapped_fields: Dict[str, Any] = field(default_factory=dict)
    record_id: str = ""
    error: str = ""
    skipped: bool = False
    retryable: bool = False
    raw_response: Any = None


@dataclass
class FeishuFieldInfo:
    field_id: str
    field_name: str
    field_type: Any = None
    is_primary: bool = False
    raw: Dict[str, Any] = field(default_factory=dict)


@dataclass
class FeishuFieldProbeResult:
    success: bool
    app_token: str = ""
    table_id: str = ""
    fields: List[FeishuFieldInfo] = field(default_factory=list)
    error: str = ""
    raw_response: Any = None


class FeishuActivityWriter:
    def __init__(self, mapping: FeishuActivityMapping):
        self.mapping = mapping

    @classmethod
    def from_env(cls) -> "FeishuActivityWriter":
        return cls(FeishuActivityMapping.load(os.getenv("QIWE_ACTIVITY_FEISHU_MAPPING", "").strip()))

    def map_fields(self, internal_fields: Dict[str, Any]) -> Dict[str, Any]:
        mapped: Dict[str, Any] = {}
        for internal_name, feishu_name in self.mapping.fields.items():
            if not feishu_name or internal_name not in internal_fields:
                continue
            if internal_name not in KNOWN_INTERNAL_FIELDS:
                logger.warning("[qiwe] unknown activity field in mapping skipped internal_field=%s", internal_name)
                continue
            converted = self._convert_value(internal_name, internal_fields[internal_name])
            if converted is _SKIP_FIELD:
                continue
            mapped[feishu_name] = converted
        for feishu_name, value in self.mapping.default_fields.items():
            if feishu_name and feishu_name not in mapped:
                mapped[feishu_name] = value
        return mapped

    def write(self, internal_fields: Dict[str, Any], *, record_id: str = "") -> FeishuWriteResult:
        mapped = self.map_fields(internal_fields)
        mode = self.mapping.mode or "dry_run"
        if not self.mapping.enabled:
            return FeishuWriteResult(success=True, mode=mode, mapped_fields=mapped, skipped=True)
        errors = self.mapping.validate()
        if errors:
            return FeishuWriteResult(success=False, mode="dry_run", mapped_fields=mapped, error="; ".join(errors), retryable=False)
        if mode == "dry_run":
            return FeishuWriteResult(success=True, mode=mode, mapped_fields=mapped)
        guard_error = self._live_write_guard(internal_fields)
        if guard_error:
            return FeishuWriteResult(success=False, mode=mode, mapped_fields=mapped, error=guard_error, retryable=False)
        app_token = os.getenv(self.mapping.app_token_env, "").strip()
        table_id = os.getenv(self.mapping.table_id_env, "").strip()
        if not app_token or not table_id:
            return FeishuWriteResult(
                success=False,
                mode=mode,
                mapped_fields=mapped,
                error=f"{self.mapping.app_token_env} and {self.mapping.table_id_env} are required for live write",
                retryable=False,
            )
        return self._write_live(app_token, table_id, mapped, record_id=record_id)

    def _live_write_guard(self, internal_fields: Dict[str, Any]) -> str:
        if not _truthy(os.getenv("QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE", "")):
            return "QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE=true is required for live write"
        allowed_groups = _csv(os.getenv("QINTOPIA_FEISHU_ACTIVITY_ALLOWED_GROUPS", ""))
        source_group_id = str(internal_fields.get("source_group_id") or "").strip()
        if allowed_groups and source_group_id not in set(allowed_groups):
            return f"source_group_id {source_group_id or '<empty>'} is not allowed for Feishu live write"
        return ""

    def probe_fields(self) -> FeishuFieldProbeResult:
        app_token = os.getenv(self.mapping.app_token_env, "").strip()
        table_id = os.getenv(self.mapping.table_id_env, "").strip()
        if not app_token or not table_id:
            return FeishuFieldProbeResult(
                success=False,
                error=f"{self.mapping.app_token_env} and {self.mapping.table_id_env} are required for field probe",
            )
        token = self._tenant_access_token()
        if not token:
            return FeishuFieldProbeResult(success=False, app_token=app_token, table_id=table_id, error="missing Feishu tenant access token")
        path = f"/bitable/v1/apps/{app_token}/tables/{table_id}/fields?page_size=200"
        try:
            response = self._request(path, token, {}, method="GET", body=False)
        except RuntimeError as exc:
            return FeishuFieldProbeResult(success=False, app_token=app_token, table_id=table_id, error=str(exc))
        code = response.get("code") if isinstance(response, dict) else None
        if code not in (0, None):
            return FeishuFieldProbeResult(success=False, app_token=app_token, table_id=table_id, error=str(response), raw_response=response)
        data = response.get("data", {}) if isinstance(response, dict) else {}
        items = data.get("items", []) if isinstance(data, dict) else []
        fields = []
        for item in items:
            if not isinstance(item, dict):
                continue
            fields.append(
                FeishuFieldInfo(
                    field_id=str(item.get("field_id") or ""),
                    field_name=str(item.get("field_name") or item.get("name") or ""),
                    field_type=item.get("type"),
                    is_primary=bool(item.get("is_primary", False)),
                    raw=dict(item),
                )
            )
        return FeishuFieldProbeResult(success=True, app_token=app_token, table_id=table_id, fields=fields, raw_response=response)

    def _convert_value(self, internal_name: str, value: Any) -> Any:
        if internal_name == "status":
            return self._convert_status_value(value)
        field_type = self.mapping.field_types.get(internal_name, "")
        if field_type == "number":
            try:
                return int(value)
            except (TypeError, ValueError):
                try:
                    return float(value)
                except (TypeError, ValueError):
                    return value
        if field_type == "text_list":
            if isinstance(value, list):
                return "\n".join(str(item) for item in value)
            return value
        if field_type == "datetime":
            return self._convert_datetime_value(value)
        if isinstance(value, list):
            return "\n".join(str(item) for item in value)
        return value

    def _convert_status_value(self, value: Any) -> Any:
        status = str(value or "").strip().lower()
        if status == "active":
            return "待执行"
        if status == "cancelled":
            return "已取消"
        return "待修正"

    def _convert_datetime_value(self, value: Any) -> Any:
        if value in (None, ""):
            return value
        if isinstance(value, (int, float)):
            return value
        parsed: datetime | None
        if isinstance(value, datetime):
            parsed = value
        elif isinstance(value, date):
            parsed = datetime.combine(value, time.min)
        else:
            parsed = self._parse_datetime_string(str(value))
        if parsed is None:
            return _SKIP_FIELD
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=ZoneInfo(os.getenv("QIWE_ACTIVITY_TIMEZONE", "Asia/Shanghai")))
        return int(parsed.astimezone(timezone.utc).timestamp() * 1000)

    def _parse_datetime_string(self, value: str) -> datetime | None:
        text = str(value or "").strip()
        if not text:
            return None
        try:
            return datetime.fromisoformat(text)
        except ValueError:
            pass
        for fmt in (
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M",
            "%Y-%m-%d",
            "%Y/%m/%d %H:%M:%S",
            "%Y/%m/%d %H:%M",
            "%Y/%m/%d",
        ):
            try:
                return datetime.strptime(text, fmt)
            except ValueError:
                continue
        return None

    def _write_live(self, app_token: str, table_id: str, fields: Dict[str, Any], *, record_id: str = "") -> FeishuWriteResult:
        token = self._tenant_access_token()
        if not token:
            return FeishuWriteResult(success=False, mode="live", mapped_fields=fields, error="missing Feishu tenant access token", retryable=False)
        if not record_id:
            lookup = self._lookup_record_id_by_upsert_key(app_token, table_id, token, fields)
            if not lookup.success:
                lookup.mapped_fields = fields
                return lookup
            record_id = lookup.record_id
        method = "PUT" if record_id else "POST"
        suffix = f"/{record_id}" if record_id else ""
        path = f"/bitable/v1/apps/{app_token}/tables/{table_id}/records{suffix}"
        try:
            response = self._request(path, token, {"fields": fields}, method=method)
        except RuntimeError as exc:
            return FeishuWriteResult(success=False, mode="live", mapped_fields=fields, error=str(exc), retryable=True)
        code = response.get("code") if isinstance(response, dict) else None
        if code not in (0, None):
            return FeishuWriteResult(success=False, mode="live", mapped_fields=fields, error=str(response), retryable=True, raw_response=response)
        data = response.get("data", {}) if isinstance(response, dict) else {}
        new_record_id = record_id or str(data.get("record", {}).get("record_id") or data.get("record_id") or "")
        return FeishuWriteResult(success=True, mode="live", mapped_fields=fields, record_id=new_record_id, raw_response=response)

    def _lookup_record_id_by_upsert_key(self, app_token: str, table_id: str, token: str, fields: Dict[str, Any]) -> FeishuWriteResult:
        upsert_field = self.mapping.fields.get(self.mapping.upsert_key, "")
        upsert_value = fields.get(upsert_field)
        if not upsert_field or upsert_value in (None, ""):
            return FeishuWriteResult(success=False, mode="live", error="upsert field value is required for live write", retryable=False)
        filter_payload = {
            "filter": {
                "conjunction": "and",
                "conditions": [
                    {
                        "field_name": upsert_field,
                        "operator": "is",
                        "value": [str(upsert_value)],
                    }
                ],
            },
            "page_size": 10,
        }
        path = f"/bitable/v1/apps/{app_token}/tables/{table_id}/records/search"
        try:
            response = self._request(path, token, filter_payload, method="POST")
        except RuntimeError as exc:
            return FeishuWriteResult(success=False, mode="live", error=str(exc), retryable=True)
        code = response.get("code") if isinstance(response, dict) else None
        if code not in (0, None):
            return FeishuWriteResult(success=False, mode="live", error=str(response), retryable=True, raw_response=response)
        data = response.get("data", {}) if isinstance(response, dict) else {}
        items = data.get("items", []) if isinstance(data, dict) else []
        if not isinstance(items, list):
            items = []
        if len(items) > 1:
            return FeishuWriteResult(
                success=False,
                mode="live",
                error=f"multiple Feishu records matched {upsert_field}={upsert_value}",
                retryable=False,
                raw_response=response,
            )
        if not items:
            return FeishuWriteResult(success=True, mode="live", raw_response=response)
        item = items[0] if isinstance(items[0], dict) else {}
        record_id = str(item.get("record_id") or "")
        if not record_id:
            return FeishuWriteResult(success=False, mode="live", error="matched Feishu record missing record_id", retryable=True, raw_response=response)
        return FeishuWriteResult(success=True, mode="live", record_id=record_id, raw_response=response)

    def _tenant_access_token(self) -> str:
        token = os.getenv("QINTOPIA_FEISHU_ACTIVITY_TENANT_ACCESS_TOKEN") or os.getenv("FEISHU_TENANT_ACCESS_TOKEN")
        if token:
            return token.strip()
        app_id, app_secret = self._activity_app_credentials()
        if not app_id or not app_secret:
            return ""
        try:
            response = self._request(
                "/auth/v3/tenant_access_token/internal",
                "",
                {"app_id": app_id, "app_secret": app_secret},
                method="POST",
                authorized=False,
            )
        except RuntimeError as exc:
            logger.warning("[qiwe] Feishu tenant token request failed: %s", exc)
            return ""
        if response.get("code") != 0:
            logger.warning("[qiwe] Feishu tenant token rejected: %s", response)
            return ""
        return str(response.get("tenant_access_token") or "").strip()

    def _activity_app_credentials(self) -> tuple[str, str]:
        app_id = os.getenv("QINTOPIA_FEISHU_ACTIVITY_APP_ID", "").strip()
        app_secret = os.getenv("QINTOPIA_FEISHU_ACTIVITY_APP_SECRET", "").strip()
        if app_id and app_secret:
            return app_id, app_secret
        if os.getenv("QINTOPIA_FEISHU_ACTIVITY_USE_HERMES_CONFIG", "").strip().lower() not in {"1", "true", "yes", "on"}:
            return app_id, app_secret
        config_path = os.getenv("QINTOPIA_FEISHU_ACTIVITY_HERMES_CONFIG", "").strip()
        if not config_path:
            hermes_home = os.getenv("HERMES_HOME", "").strip()
            if hermes_home:
                config_path = str(Path(hermes_home).expanduser() / "config.yaml")
        if not config_path:
            return app_id, app_secret
        try:
            fallback = _read_simple_yaml_section(Path(config_path).expanduser(), "feishu")
        except OSError as exc:
            logger.warning("[qiwe] Feishu Hermes config fallback unavailable: %s", exc)
            return app_id, app_secret
        return app_id or fallback.get("app_id", ""), app_secret or fallback.get("app_secret", "")

    def _request(self, path: str, token: str, payload: Dict[str, Any], *, method: str, authorized: bool = True, body: bool = True) -> Dict[str, Any]:
        base = os.getenv("FEISHU_API_BASE", "https://open.feishu.cn/open-apis").rstrip("/")
        headers = {"Content-Type": "application/json; charset=utf-8"}
        if authorized:
            headers["Authorization"] = f"Bearer {token}"
        request = urllib.request.Request(
            base + path,
            data=json.dumps(payload, ensure_ascii=False).encode("utf-8") if body else None,
            headers=headers,
            method=method,
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as exc:
            body = exc.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"Feishu HTTP {exc.code}: {body[:240]}") from exc
        except urllib.error.URLError as exc:
            raise RuntimeError(f"Feishu request failed: {exc.reason}") from exc
        except json.JSONDecodeError as exc:
            raise RuntimeError("Feishu response was not valid JSON") from exc


def _read_simple_yaml_section(path: Path, section: str) -> Dict[str, str]:
    values: Dict[str, str] = {}
    lines = path.read_text(encoding="utf-8").splitlines()
    in_section = False
    section_prefix = f"{section}:"
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if not line.startswith((" ", "\t")) and stripped.endswith(":"):
            in_section = stripped == section_prefix
            continue
        if not in_section:
            continue
        if not line.startswith((" ", "\t")):
            break
        if ":" not in stripped:
            continue
        key, value = stripped.split(":", 1)
        value = value.strip().strip("'\"")
        if value and value.lower() not in {"null", "none", "~"}:
            values[key.strip()] = value
    return values


def _csv(value: Any) -> List[str]:
    return [part.strip() for part in str(value or "").split(",") if part.strip()]


def _truthy(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    return str(value or "").strip().lower() in {"1", "true", "yes", "on", "是"}
