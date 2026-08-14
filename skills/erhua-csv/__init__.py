"""Append-only, group-isolated CSV storage for Erhua QiWe conversations."""

from __future__ import annotations

import csv
import errno
import fcntl
import hashlib
import io
import json
import os
import re
import shutil
import stat
import uuid
from contextlib import contextmanager
from datetime import date, datetime, timezone
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any, Iterator, NamedTuple
from zoneinfo import ZoneInfo


TOOL_LIST = "qintopia_erhua_csv_list"
TOOL_CREATE = "qintopia_erhua_csv_create"
TOOL_APPEND = "qintopia_erhua_csv_append"
TOOL_QUERY = "qintopia_erhua_csv_query"

ASIA_SHANGHAI = ZoneInfo("Asia/Shanghai")
SYSTEM_COLUMNS = (
    "_event_id",
    "_created_at",
    "_actor_user_id",
    "_actor_name",
    "_source_message_id",
    "_row_sha256",
)
EXTRA_COLUMN = "_extra_json"
FIELD_TYPES = {"text", "decimal", "integer", "boolean", "date", "datetime", "enum"}
FORMULA_PREFIXES = ("=", "+", "-", "@", "\t", "\r")
FIELD_NAME_RE = re.compile(r"^[^\x00-\x1f\x7f,/\\]{1,64}$")
CSV_ID_RE = re.compile(r"^[0-9a-f]{32}$")
EVENT_ID_RE = re.compile(r"^[0-9a-f]{32}$")
DATE_DIR_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

MAX_DATASETS_PER_GROUP = 20
MAX_VERSIONS_PER_DATASET = 5
MAX_FIELDS_PER_SCHEMA = 32
MAX_ROW_BYTES = 16 * 1024
MAX_FILE_BYTES = 10 * 1024 * 1024
MAX_GROUP_BYTES = 100 * 1024 * 1024
MAX_QUERY_ROWS = 100
MAX_QUERY_FILTERS = 5
MAX_DECIMAL_INPUT_CHARS = 1024
MAX_DECIMAL_FIXED_DIGITS = 1024
SNAPSHOT_RETENTION_DAYS = 30

LEDGER_FIELDS = (
    {"name": "occurred_at", "type": "datetime", "required": True},
    {"name": "account", "type": "text", "required": True, "default": "cash"},
    {"name": "currency", "type": "text", "required": True, "default": "CNY"},
    {
        "name": "direction",
        "type": "enum",
        "required": True,
        "enum": ["income", "expense"],
    },
    {"name": "amount", "type": "decimal", "required": True},
    {"name": "amount_delta", "type": "decimal", "required": True, "generated": True},
    {"name": "category", "type": "text", "required": False},
    {"name": "note", "type": "text", "required": False},
    {"name": "reverses_event_id", "type": "text", "required": False},
)


QINTOPIA_ERHUA_CSV_LIST_SCHEMA = {
    "description": (
        "List append-only CSV datasets visible to the current QiWe group. Pass csv_id "
        "to inspect its fields and versions. Group scope is supplied by the runtime."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "csv_id": {
                "type": "string",
                "description": "Opaque dataset id returned by this tool. Never a path or filename.",
            }
        },
        "additionalProperties": False,
    },
}

FIELD_SCHEMA = {
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "type": {"type": "string", "enum": sorted(FIELD_TYPES)},
        "required": {"type": "boolean"},
        "description": {"type": "string"},
        "enum": {"type": "array", "items": {"type": "string"}, "minItems": 1},
        "default": {},
    },
    "required": ["name", "type"],
    "additionalProperties": False,
}

QINTOPIA_ERHUA_CSV_CREATE_SCHEMA = {
    "description": (
        "Create a custom or ledger CSV for the current QiWe group. To add formal "
        "optional columns later, pass version_of with only the new fields. Existing "
        "fields and rows are immutable."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "name": {"type": "string", "description": "Group-visible dataset name."},
            "description": {"type": "string"},
            "preset": {"type": "string", "enum": ["custom", "ledger"]},
            "fields": {"type": "array", "items": FIELD_SCHEMA, "maxItems": MAX_FIELDS_PER_SCHEMA},
            "version_of": {
                "type": "string",
                "description": "Existing csv_id when adding optional formal fields.",
            },
        },
        "required": ["name"],
        "additionalProperties": False,
    },
}

QINTOPIA_ERHUA_CSV_APPEND_SCHEMA = {
    "description": (
        "Append one auditable row to a current-group CSV. Unknown business fields are "
        "retained as extra data. System fields and generated ledger fields are forbidden. "
        "For a ledger reversal, provide reverses_event_id."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "csv_id": {"type": "string"},
            "row": {"type": "object"},
        },
        "required": ["csv_id", "row"],
        "additionalProperties": False,
    },
}

QINTOPIA_ERHUA_CSV_QUERY_SCHEMA = {
    "description": (
        "Query the current group's logical CSV across all schema versions. Supports up "
        "to five equality filters, bounded pagination, count, and exact Decimal sum."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "csv_id": {"type": "string"},
            "filters": {"type": "object", "maxProperties": MAX_QUERY_FILTERS},
            "offset": {"type": "integer", "minimum": 0},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_QUERY_ROWS},
            "count": {"type": "boolean"},
            "sum_field": {"type": "string"},
        },
        "required": ["csv_id"],
        "additionalProperties": False,
    },
}


class CsvWorkspaceError(RuntimeError):
    """A fail-closed, user-safe workspace error."""


class CsvIntegrityError(CsvWorkspaceError):
    """Persistent data failed append-only integrity validation."""


class SessionContext(NamedTuple):
    platform: str
    chat_id: str
    user_id: str
    user_name: str
    message_id: str

    @classmethod
    def current(cls) -> "SessionContext":
        return cls(
            platform=_session_env("HERMES_SESSION_PLATFORM"),
            chat_id=_session_env("HERMES_SESSION_CHAT_ID"),
            user_id=_session_env("HERMES_SESSION_USER_ID"),
            user_name=_session_env("HERMES_SESSION_USER_NAME"),
            message_id=_session_env("HERMES_SESSION_MESSAGE_ID"),
        )

    def validate_group(self) -> None:
        if self.platform != "qiwe":
            raise CsvWorkspaceError("Erhua CSV is available only in QiWe group chats")
        if not self.chat_id or not self.user_id or not self.message_id:
            raise CsvWorkspaceError("trusted QiWe group context is incomplete")
        if self.chat_id == self.user_id:
            raise CsvWorkspaceError("Erhua CSV is not available in direct chats")
        for label, value in (
            ("chat id", self.chat_id),
            ("user id", self.user_id),
            ("message id", self.message_id),
        ):
            if len(value.encode("utf-8")) > 512 or "\x00" in value:
                raise CsvWorkspaceError(f"trusted {label} is invalid")
            _reject_formula_text(value, f"trusted {label}")
        _reject_formula_text(self.user_name, "actor name")


def _session_env(name: str) -> str:
    try:
        from gateway.session_context import get_session_env

        value = get_session_env(name, "")
    except Exception:
        value = os.getenv(name, "")
    return str(value or "").strip()


def _json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def _json_result(value: dict[str, Any]) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _iso_now() -> str:
    return _now().isoformat(timespec="microseconds").replace("+00:00", "Z")


def _parse_timestamp(value: str) -> datetime:
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise CsvIntegrityError("stored timestamp is invalid") from exc
    if parsed.tzinfo is None:
        raise CsvIntegrityError("stored timestamp lacks a timezone")
    return parsed.astimezone(timezone.utc)


def _reject_formula_text(value: Any, label: str) -> None:
    if value is None:
        return
    text = str(value)
    if text.startswith(FORMULA_PREFIXES):
        raise CsvWorkspaceError(f"{label} begins with a spreadsheet formula prefix")


def _safe_visible_text(value: Any, label: str, maximum: int, *, required: bool = False) -> str:
    if not isinstance(value, str):
        if required:
            raise CsvWorkspaceError(f"{label} must be text")
        value = "" if value is None else str(value)
    text = value.strip()
    if required and not text:
        raise CsvWorkspaceError(f"{label} is required")
    if len(text.encode("utf-8")) > maximum:
        raise CsvWorkspaceError(f"{label} is too long")
    if "\x00" in text:
        raise CsvWorkspaceError(f"{label} contains a null byte")
    _reject_formula_text(text, label)
    return text


def _validate_csv_id(value: Any) -> str:
    csv_id = str(value or "").strip().lower()
    if not CSV_ID_RE.fullmatch(csv_id):
        raise CsvWorkspaceError("csv_id is invalid")
    return csv_id


def _validate_event_id(value: Any) -> str:
    event_id = str(value or "").strip().lower()
    if not EVENT_ID_RE.fullmatch(event_id):
        raise CsvWorkspaceError("reverses_event_id is invalid")
    return event_id


def _canonical_decimal(value: Any, label: str) -> str:
    if isinstance(value, bool) or value is None:
        raise CsvWorkspaceError(f"{label} must be a decimal")
    text = str(value).strip()
    if not text or len(text) > MAX_DECIMAL_INPUT_CHARS:
        raise CsvWorkspaceError(f"{label} exceeds the supported decimal size")
    try:
        decimal_value = Decimal(text)
    except (InvalidOperation, ValueError) as exc:
        raise CsvWorkspaceError(f"{label} must be a decimal") from exc
    if not decimal_value.is_finite():
        raise CsvWorkspaceError(f"{label} must be finite")
    _, digits, exponent = decimal_value.as_tuple()
    integer_digits = max(len(digits) + exponent, 1)
    fractional_digits = max(-exponent, 0)
    if (
        len(digits) > MAX_DECIMAL_FIXED_DIGITS
        or integer_digits + fractional_digits > MAX_DECIMAL_FIXED_DIGITS
    ):
        raise CsvWorkspaceError(f"{label} exceeds the supported decimal size")
    normalized = format(decimal_value, "f")
    if "." in normalized:
        normalized = normalized.rstrip("0").rstrip(".")
    return normalized or "0"


def _canonical_datetime(value: Any, label: str) -> str:
    text = _safe_visible_text(value, label, 80, required=True)
    try:
        parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
    except ValueError as exc:
        raise CsvWorkspaceError(f"{label} must be an ISO 8601 datetime") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=ASIA_SHANGHAI)
    return parsed.isoformat(timespec="seconds")


def _canonical_date(value: Any, label: str) -> str:
    text = _safe_visible_text(value, label, 32, required=True)
    try:
        return date.fromisoformat(text).isoformat()
    except ValueError as exc:
        raise CsvWorkspaceError(f"{label} must be an ISO 8601 date") from exc


def _canonical_field_value(field: dict[str, Any], value: Any) -> str:
    name = field["name"]
    field_type = field["type"]
    if value is None or value == "":
        if "default" in field:
            value = field["default"]
        elif field.get("required"):
            raise CsvWorkspaceError(f"field {name} is required")
        else:
            return ""
    if field_type == "text":
        return _safe_visible_text(value, f"field {name}", 4096)
    if field_type == "decimal":
        return _canonical_decimal(value, f"field {name}")
    if field_type == "integer":
        if isinstance(value, bool):
            raise CsvWorkspaceError(f"field {name} must be an integer")
        try:
            integer = int(str(value))
        except (TypeError, ValueError) as exc:
            raise CsvWorkspaceError(f"field {name} must be an integer") from exc
        if str(value).strip() not in {str(integer), f"+{integer}"}:
            raise CsvWorkspaceError(f"field {name} must be an integer")
        if integer < 0:
            raise CsvWorkspaceError(f"field {name} cannot be negative; use decimal for signed values")
        return str(integer)
    if field_type == "boolean":
        if not isinstance(value, bool):
            raise CsvWorkspaceError(f"field {name} must be a boolean")
        return "true" if value else "false"
    if field_type == "date":
        return _canonical_date(value, f"field {name}")
    if field_type == "datetime":
        return _canonical_datetime(value, f"field {name}")
    if field_type == "enum":
        text = _safe_visible_text(value, f"field {name}", 256, required=True)
        if text not in field["enum"]:
            raise CsvWorkspaceError(f"field {name} is not an allowed enum value")
        return text
    raise CsvWorkspaceError(f"field {name} has an unsupported type")


def _public_value(field_type: str, value: str) -> Any:
    if value == "":
        return None
    if field_type == "boolean":
        return value == "true"
    if field_type == "integer":
        return int(value)
    return value


def _normalize_fields(raw_fields: Any, *, added_version: bool = False) -> list[dict[str, Any]]:
    if raw_fields is None:
        raw_fields = []
    if not isinstance(raw_fields, list):
        raise CsvWorkspaceError("fields must be a list")
    if len(raw_fields) > MAX_FIELDS_PER_SCHEMA:
        raise CsvWorkspaceError("schema has too many business fields")
    fields: list[dict[str, Any]] = []
    names: set[str] = set()
    for raw in raw_fields:
        if not isinstance(raw, dict):
            raise CsvWorkspaceError("each field must be an object")
        unexpected = set(raw) - {"name", "type", "required", "description", "enum", "default"}
        if unexpected:
            raise CsvWorkspaceError("field contains unsupported properties")
        name = str(raw.get("name") or "").strip()
        if not FIELD_NAME_RE.fullmatch(name) or name.startswith("_"):
            raise CsvWorkspaceError("field name is invalid")
        _reject_formula_text(name, "field name")
        if name in names or name in SYSTEM_COLUMNS or name == EXTRA_COLUMN:
            raise CsvWorkspaceError(f"field {name} is duplicated or reserved")
        field_type = str(raw.get("type") or "").strip()
        if field_type not in FIELD_TYPES:
            raise CsvWorkspaceError(f"field {name} has an unsupported type")
        required = bool(raw.get("required", False))
        if added_version and required:
            raise CsvWorkspaceError("new version fields must be optional")
        field: dict[str, Any] = {"name": name, "type": field_type, "required": required}
        description = _safe_visible_text(raw.get("description"), f"field {name} description", 500)
        if description:
            field["description"] = description
        if field_type == "enum":
            enum_values = raw.get("enum")
            if not isinstance(enum_values, list) or not 1 <= len(enum_values) <= 100:
                raise CsvWorkspaceError(f"field {name} requires enum values")
            cleaned = [_safe_visible_text(item, f"field {name} enum", 256, required=True) for item in enum_values]
            if len(set(cleaned)) != len(cleaned):
                raise CsvWorkspaceError(f"field {name} enum values are duplicated")
            field["enum"] = cleaned
        elif "enum" in raw:
            raise CsvWorkspaceError(f"field {name} enum is allowed only for enum fields")
        if "default" in raw:
            field["default"] = raw["default"]
            _canonical_field_value(field, raw["default"])
        fields.append(field)
        names.add(name)
    return fields


def _event_checksum(event: dict[str, Any]) -> str:
    return _sha256(_json_bytes({key: value for key, value in event.items() if key != "_event_sha256"}))


def _schema_checksum(schema: dict[str, Any]) -> str:
    return _sha256(_json_bytes({key: value for key, value in schema.items() if key != "_schema_sha256"}))


def _row_checksum(row: dict[str, str], header: list[str]) -> str:
    return _sha256(_json_bytes({column: row.get(column, "") for column in header if column != "_row_sha256"}))


def _csv_line(values: list[str]) -> bytes:
    output = io.StringIO(newline="")
    csv.writer(output, lineterminator="\n").writerow(values)
    return output.getvalue().encode("utf-8")


def _regular_file(path: Path, *, max_bytes: int | None = None) -> os.stat_result:
    try:
        info = path.lstat()
    except FileNotFoundError as exc:
        raise CsvIntegrityError("required workspace file is missing") from exc
    if not stat.S_ISREG(info.st_mode) or path.is_symlink():
        raise CsvIntegrityError("workspace contains a non-regular file")
    if stat.S_IMODE(info.st_mode) != 0o600:
        raise CsvIntegrityError("workspace file permissions are unsafe")
    if info.st_nlink != 1:
        raise CsvIntegrityError("workspace file has an unsafe hard link")
    if max_bytes is not None and info.st_size > max_bytes:
        raise CsvIntegrityError("workspace file exceeds its size limit")
    return info


@contextmanager
def _open_text_nofollow(path: Path) -> Iterator[io.TextIOWrapper]:
    expected = _regular_file(path)
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        fd = os.open(path, flags)
    except OSError as exc:
        if exc.errno == errno.ELOOP:
            raise CsvIntegrityError("workspace file is a symlink") from exc
        raise
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise CsvIntegrityError("workspace path is not a regular file")
        if (info.st_dev, info.st_ino) != (expected.st_dev, expected.st_ino):
            raise CsvIntegrityError("workspace file changed while opening")
        with os.fdopen(fd, "r", encoding="utf-8", newline="", closefd=False) as handle:
            yield handle
    finally:
        os.close(fd)


def _write_all(fd: int, payload: bytes) -> None:
    remaining = memoryview(payload)
    while remaining:
        try:
            written = os.write(fd, remaining)
        except InterruptedError:
            continue
        if written <= 0:
            raise CsvIntegrityError("append produced a partial write")
        remaining = remaining[written:]


class CsvWorkspace:
    def __init__(self, root: Path | None = None, internal_root: Path | None = None):
        profile_data = Path.home() / ".hermes" / "profiles" / "erhua" / "data"
        self.root = Path(root) if root is not None else profile_data / "csv" / "v1"
        self.internal_root = (
            Path(internal_root) if internal_root is not None else profile_data / "csv-internal" / "v1"
        )

    def list(self, context: SessionContext, csv_id: str = "") -> dict[str, Any]:
        context.validate_group()
        group = self._group_path(context)
        if not group.exists():
            return {"success": True, "datasets": [], "dataset_count": 0}
        self._validate_group_ancestors(group)
        self._validate_managed_tree(group)
        with self._lock(group / "locks" / "group.lock", exclusive=False):
            catalog = self._read_catalog(group)
            logical = self._logical_catalog(catalog)
            if csv_id:
                target = logical.get(_validate_csv_id(csv_id))
                if target is None:
                    raise CsvWorkspaceError("csv_id was not found in the current group")
                schemas = self._load_all_schemas(group, target)
                return {
                    "success": True,
                    "dataset": self._public_dataset(group, target, schemas),
                }
            datasets = [
                self._public_dataset(group, item, self._load_all_schemas(group, item), include_fields=False)
                for item in logical.values()
            ]
            datasets.sort(key=lambda item: (item["name"], item["csv_id"]))
            return {"success": True, "datasets": datasets, "dataset_count": len(datasets)}

    def create(self, context: SessionContext, args: dict[str, Any]) -> dict[str, Any]:
        context.validate_group()
        if not isinstance(args, dict):
            raise CsvWorkspaceError("create arguments must be an object")
        name = _safe_visible_text(args.get("name"), "name", 160, required=True)
        description = _safe_visible_text(args.get("description"), "description", 1200)
        preset = str(args.get("preset") or "custom").strip()
        if preset not in {"custom", "ledger"}:
            raise CsvWorkspaceError("preset must be custom or ledger")
        version_of = str(args.get("version_of") or "").strip()
        group = self._ensure_group(context)
        with self._lock(group / "locks" / "group.lock", exclusive=True):
            self._validate_managed_tree(group)
            catalog = self._read_catalog(group)
            logical = self._logical_catalog(catalog)
            if version_of:
                result = self._create_version(
                    group,
                    context,
                    logical,
                    _validate_csv_id(version_of),
                    name,
                    description,
                    preset,
                    args.get("fields"),
                )
            else:
                result = self._create_dataset(
                    group, context, logical, name, description, preset, args.get("fields")
                )
            return self._result_after_snapshot(result, group, context)

    def append(self, context: SessionContext, csv_id: str, raw_row: Any) -> dict[str, Any]:
        context.validate_group()
        csv_id = _validate_csv_id(csv_id)
        if not isinstance(raw_row, dict):
            raise CsvWorkspaceError("row must be an object")
        if any(key in SYSTEM_COLUMNS or key == EXTRA_COLUMN for key in raw_row):
            raise CsvWorkspaceError("row cannot supply system fields")
        group = self._ensure_group(context)
        with self._lock(group / "locks" / "group.lock", exclusive=True):
            self._validate_managed_tree(group)
            target = self._logical_catalog(self._read_catalog(group)).get(csv_id)
            if target is None:
                raise CsvWorkspaceError("csv_id was not found in the current group")
            version = target["latest_version"]
            with self._lock(group / "locks" / f"{csv_id}.lock", exclusive=True):
                schema = self._load_schema(group, csv_id, version)
                rows_path = self._rows_path(group, csv_id, version)
                existing = self._read_rows(rows_path, schema)
                event_id = self._idempotent_event_id(context, csv_id, version, raw_row)
                duplicate = next((item for item in existing if item["_event_id"] == event_id), None)
                if duplicate is not None:
                    result = self._append_result(group, target, schema, duplicate, idempotent=True)
                    return self._result_after_snapshot(result, group, context)
                canonical_row = dict(raw_row)
                if target["preset"] == "ledger":
                    canonical_row = self._prepare_ledger_row(group, target, schema, existing, raw_row)
                durable = self._prepare_durable_row(context, schema, canonical_row, event_id)
                self._check_group_quota(group, extra_bytes=len(self._encode_row(schema, durable)))
                self._append_row(rows_path, schema, durable)
                verified_rows = self._read_rows(rows_path, schema)
                verified = next((item for item in verified_rows if item["_event_id"] == durable["_event_id"]), None)
                if verified is None or verified["_row_sha256"] != durable["_row_sha256"]:
                    raise CsvIntegrityError("appended row could not be verified")
                result = self._append_result(group, target, schema, verified, idempotent=False)
                return self._result_after_snapshot(result, group, context)

    def query(self, context: SessionContext, args: dict[str, Any]) -> dict[str, Any]:
        context.validate_group()
        csv_id = _validate_csv_id(args.get("csv_id"))
        filters = args.get("filters") or {}
        if not isinstance(filters, dict) or len(filters) > MAX_QUERY_FILTERS:
            raise CsvWorkspaceError("filters must contain at most five equality filters")
        offset = args.get("offset", 0)
        limit = args.get("limit", MAX_QUERY_ROWS)
        if isinstance(offset, bool) or not isinstance(offset, int) or offset < 0:
            raise CsvWorkspaceError("offset must be a non-negative integer")
        if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= MAX_QUERY_ROWS:
            raise CsvWorkspaceError("limit must be between 1 and 100")
        group = self._group_path(context)
        if not group.exists():
            raise CsvWorkspaceError("csv_id was not found in the current group")
        self._validate_group_ancestors(group)
        self._validate_managed_tree(group)
        with self._lock(group / "locks" / "group.lock", exclusive=False):
            target = self._logical_catalog(self._read_catalog(group)).get(csv_id)
            if target is None:
                raise CsvWorkspaceError("csv_id was not found in the current group")
            with self._lock(group / "locks" / f"{csv_id}.lock", exclusive=False):
                schemas = self._load_all_schemas(group, target)
                rows = self._logical_rows(group, target, schemas)
        normalized_filters = self._normalize_filters(filters, schemas)
        matched = [row for row in rows if self._matches(row, normalized_filters)]
        page = matched[offset : offset + limit]
        result: dict[str, Any] = {
            "success": True,
            "csv_id": csv_id,
            "name": target["name"],
            "rows": page,
            "offset": offset,
            "limit": limit,
            "returned": len(page),
            "has_more": offset + len(page) < len(matched),
        }
        if bool(args.get("count")):
            result["count"] = len(matched)
        sum_field = str(args.get("sum_field") or "").strip()
        if sum_field:
            field = self._field_map(schemas).get(sum_field)
            if field is None or field["type"] != "decimal":
                raise CsvWorkspaceError("sum_field must name a declared decimal field")
            total = sum(Decimal(str(row["fields"][sum_field])) for row in matched if row["fields"].get(sum_field) is not None)
            result["sum"] = {"field": sum_field, "value": _canonical_decimal(total, "sum")}
        return result

    def _group_path(self, context: SessionContext) -> Path:
        scope = _sha256(f"qiwe\0{context.chat_id}".encode("utf-8"))
        return self.root / "groups" / scope

    def _ensure_group(self, context: SessionContext) -> Path:
        group = self._group_path(context)
        for path in (self.root, self.root / "groups", group, group / "datasets", group / "locks"):
            self._ensure_dir(path)
        self._validate_group_ancestors(group)
        catalog = group / "catalog.jsonl"
        if not catalog.exists():
            self._create_file(catalog, b"")
        return group

    def _ensure_dir(self, path: Path) -> None:
        if path.exists() or path.is_symlink():
            info = path.lstat()
            if path.is_symlink() or not stat.S_ISDIR(info.st_mode):
                raise CsvIntegrityError("managed workspace directory is unsafe")
            if stat.S_IMODE(info.st_mode) != 0o700:
                raise CsvIntegrityError("managed workspace directory permissions are unsafe")
            return
        parent = path.parent
        if parent != path and not parent.exists():
            self._ensure_dir(parent)
        try:
            path.mkdir(mode=0o700)
        except FileExistsError:
            pass
        info = path.lstat()
        if path.is_symlink() or not stat.S_ISDIR(info.st_mode):
            raise CsvIntegrityError("managed workspace directory is unsafe")
        if stat.S_IMODE(info.st_mode) != 0o700:
            raise CsvIntegrityError("managed workspace directory permissions are unsafe")

    def _create_file(self, path: Path, payload: bytes) -> None:
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            fd = os.open(path, flags, 0o600)
        except FileExistsError:
            _regular_file(path)
            return
        try:
            if payload:
                _write_all(fd, payload)
            os.fsync(fd)
        finally:
            os.close(fd)
        os.chmod(path, 0o600)
        self._fsync_dir(path.parent)

    @staticmethod
    def _fsync_dir(path: Path) -> None:
        flags = os.O_RDONLY
        if hasattr(os, "O_DIRECTORY"):
            flags |= os.O_DIRECTORY
        fd = os.open(path, flags)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)

    @contextmanager
    def _lock(self, path: Path, *, exclusive: bool) -> Iterator[None]:
        self._ensure_dir(path.parent)
        flags = os.O_RDWR | os.O_CREAT
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            fd = os.open(path, flags, 0o600)
        except OSError as exc:
            if exc.errno == errno.ELOOP:
                raise CsvIntegrityError("workspace lock path is a symlink") from exc
            raise
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise CsvIntegrityError("workspace lock is not a regular file")
            os.fchmod(fd, 0o600)
            fcntl.flock(fd, fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH)
            yield
        finally:
            try:
                fcntl.flock(fd, fcntl.LOCK_UN)
            finally:
                os.close(fd)

    def _validate_managed_tree(self, group: Path) -> None:
        if group.is_symlink() or not group.is_dir():
            raise CsvIntegrityError("group workspace is unsafe")
        for path in group.rglob("*"):
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                raise CsvIntegrityError("workspace contains a symlink")
            if not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)):
                raise CsvIntegrityError("workspace contains an unsupported file type")
            expected_mode = 0o700 if stat.S_ISDIR(info.st_mode) else 0o600
            if stat.S_IMODE(info.st_mode) != expected_mode:
                raise CsvIntegrityError("workspace permissions are unsafe")
            if stat.S_ISREG(info.st_mode) and info.st_nlink != 1:
                raise CsvIntegrityError("workspace contains an unsafe hard link")

    def _validate_group_ancestors(self, group: Path) -> None:
        for path in (self.root, self.root / "groups", group):
            try:
                info = path.lstat()
            except FileNotFoundError as exc:
                raise CsvIntegrityError("managed workspace directory is missing") from exc
            if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
                raise CsvIntegrityError("managed workspace directory is unsafe")
            if stat.S_IMODE(info.st_mode) != 0o700:
                raise CsvIntegrityError("managed workspace directory permissions are unsafe")

    def _read_catalog(self, group: Path) -> list[dict[str, Any]]:
        path = group / "catalog.jsonl"
        _regular_file(path, max_bytes=MAX_FILE_BYTES)
        events: list[dict[str, Any]] = []
        with _open_text_nofollow(path) as handle:
            for line in handle:
                if not line.endswith("\n"):
                    raise CsvIntegrityError("catalog has a partial tail")
                try:
                    event = json.loads(line)
                except json.JSONDecodeError as exc:
                    raise CsvIntegrityError("catalog contains invalid JSON") from exc
                if not isinstance(event, dict) or event.get("_event_sha256") != _event_checksum(event):
                    raise CsvIntegrityError("catalog checksum validation failed")
                events.append(event)
        return events

    def _append_catalog(self, group: Path, event: dict[str, Any]) -> None:
        event["_event_sha256"] = _event_checksum(event)
        payload = _json_bytes(event) + b"\n"
        path = group / "catalog.jsonl"
        self._check_group_quota(group, extra_bytes=len(payload))
        self._append_bytes(path, payload)

    def _append_bytes(self, path: Path, payload: bytes) -> None:
        expected = _regular_file(path, max_bytes=MAX_FILE_BYTES)
        if expected.st_size + len(payload) > MAX_FILE_BYTES:
            raise CsvWorkspaceError("target file would exceed 10 MiB")
        flags = os.O_WRONLY | os.O_APPEND
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        fd = os.open(path, flags)
        try:
            info = os.fstat(fd)
            if (
                not stat.S_ISREG(info.st_mode)
                or info.st_nlink != 1
                or (info.st_dev, info.st_ino) != (expected.st_dev, expected.st_ino)
            ):
                raise CsvIntegrityError("workspace append target changed while opening")
            _write_all(fd, payload)
            os.fsync(fd)
        finally:
            os.close(fd)

    def _logical_catalog(self, events: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
        logical: dict[str, dict[str, Any]] = {}
        for event in events:
            event_type = event.get("event")
            csv_id = event.get("csv_id")
            version = event.get("version")
            if not isinstance(csv_id, str) or not CSV_ID_RE.fullmatch(csv_id):
                raise CsvIntegrityError("catalog contains an invalid csv id")
            if not isinstance(version, int) or not 1 <= version <= MAX_VERSIONS_PER_DATASET:
                raise CsvIntegrityError("catalog contains an invalid schema version")
            if event_type == "dataset_created":
                if csv_id in logical or version != 1:
                    raise CsvIntegrityError("catalog dataset history is inconsistent")
                logical[csv_id] = {
                    "csv_id": csv_id,
                    "name": event["name"],
                    "description": event.get("description", ""),
                    "preset": event["preset"],
                    "created_at": event["created_at"],
                    "created_by": event.get("actor_name", ""),
                    "latest_version": 1,
                    "schema_events": [event],
                }
            elif event_type == "schema_version_created":
                target = logical.get(csv_id)
                if target is None or version != target["latest_version"] + 1:
                    raise CsvIntegrityError("catalog schema version history is inconsistent")
                target["latest_version"] = version
                target["schema_events"].append(event)
                if event.get("description"):
                    target["description"] = event["description"]
            else:
                raise CsvIntegrityError("catalog contains an unknown event type")
        return logical

    def _dataset_dir(self, group: Path, csv_id: str) -> Path:
        return group / "datasets" / csv_id

    def _schema_path(self, group: Path, csv_id: str, version: int) -> Path:
        return self._dataset_dir(group, csv_id) / "schemas" / f"v{version}.json"

    def _rows_path(self, group: Path, csv_id: str, version: int) -> Path:
        return self._dataset_dir(group, csv_id) / "rows" / f"v{version}.csv"

    def _load_schema(self, group: Path, csv_id: str, version: int) -> dict[str, Any]:
        path = self._schema_path(group, csv_id, version)
        _regular_file(path, max_bytes=256 * 1024)
        try:
            with _open_text_nofollow(path) as handle:
                schema = json.load(handle)
        except json.JSONDecodeError as exc:
            raise CsvIntegrityError("schema file contains invalid JSON") from exc
        if not isinstance(schema, dict) or schema.get("_schema_sha256") != _schema_checksum(schema):
            raise CsvIntegrityError("schema checksum validation failed")
        if schema.get("csv_id") != csv_id or schema.get("version") != version:
            raise CsvIntegrityError("schema identity does not match its path")
        fields = schema.get("fields")
        if not isinstance(fields, list) or len(fields) > MAX_FIELDS_PER_SCHEMA:
            raise CsvIntegrityError("stored schema fields are invalid")
        return schema

    def _load_all_schemas(self, group: Path, target: dict[str, Any]) -> list[dict[str, Any]]:
        schemas = [
            self._load_schema(group, target["csv_id"], version)
            for version in range(1, target["latest_version"] + 1)
        ]
        for index, event in enumerate(target["schema_events"]):
            if event.get("schema_sha256") != schemas[index].get("_schema_sha256"):
                raise CsvIntegrityError("catalog and schema checksum do not match")
        return schemas

    def _create_dataset(
        self,
        group: Path,
        context: SessionContext,
        logical: dict[str, dict[str, Any]],
        name: str,
        description: str,
        preset: str,
        raw_fields: Any,
    ) -> dict[str, Any]:
        if len(logical) >= MAX_DATASETS_PER_GROUP:
            raise CsvWorkspaceError("current group already has 20 CSV datasets")
        if preset == "ledger":
            if raw_fields not in (None, []):
                raise CsvWorkspaceError("ledger preset fields are generated automatically")
            fields = [dict(field) for field in LEDGER_FIELDS]
        else:
            fields = _normalize_fields(raw_fields)
            if not fields:
                raise CsvWorkspaceError("custom preset requires at least one field")
        seed = _json_bytes(
            {
                "scope": _sha256(f"qiwe\0{context.chat_id}".encode("utf-8")),
                "message": context.message_id,
                "name": name,
                "preset": preset,
                "fields": fields,
            }
        )
        csv_id = uuid.UUID(bytes=hashlib.sha256(seed).digest()[:16]).hex
        if csv_id in logical:
            target = logical[csv_id]
            return {
                "success": True,
                "idempotent": True,
                "dataset": self._public_dataset(group, target, self._load_all_schemas(group, target)),
            }
        schema = self._new_schema(csv_id, 1, name, description, preset, fields)
        self._write_new_version(group, schema)
        event = {
            "event": "dataset_created",
            "csv_id": csv_id,
            "version": 1,
            "name": name,
            "description": description,
            "preset": preset,
            "schema_sha256": schema["_schema_sha256"],
            "created_at": _iso_now(),
            "actor_user_id": context.user_id,
            "actor_name": context.user_name,
            "source_message_id": context.message_id,
        }
        self._append_catalog(group, event)
        target = self._logical_catalog(self._read_catalog(group))[csv_id]
        return {
            "success": True,
            "idempotent": False,
            "dataset": self._public_dataset(group, target, [schema]),
        }

    def _create_version(
        self,
        group: Path,
        context: SessionContext,
        logical: dict[str, dict[str, Any]],
        csv_id: str,
        name: str,
        description: str,
        preset: str,
        raw_fields: Any,
    ) -> dict[str, Any]:
        target = logical.get(csv_id)
        if target is None:
            raise CsvWorkspaceError("version_of was not found in the current group")
        if name != target["name"]:
            raise CsvWorkspaceError("schema version cannot rename a dataset")
        if preset != "custom" and preset != target["preset"]:
            raise CsvWorkspaceError("schema version cannot change the preset")
        additions = _normalize_fields(raw_fields, added_version=True)
        if not additions:
            raise CsvWorkspaceError("schema version requires at least one new optional field")
        schemas = self._load_all_schemas(group, target)
        replay_event = next(
            (
                event
                for event in target["schema_events"][1:]
                if event.get("source_message_id") == context.message_id
                and event.get("actor_user_id") == context.user_id
            ),
            None,
        )
        if replay_event is not None:
            replay_schema = schemas[replay_event["version"] - 1]
            previous_schema = schemas[replay_event["version"] - 2]
            previous_names = {field["name"] for field in previous_schema["fields"]}
            replay_additions = [
                field for field in replay_schema["fields"] if field["name"] not in previous_names
            ]
            if replay_additions != additions:
                raise CsvWorkspaceError("source message already created a different schema version")
            return {
                "success": True,
                "idempotent": True,
                "dataset": self._public_dataset(group, target, schemas),
            }
        if target["latest_version"] >= MAX_VERSIONS_PER_DATASET:
            raise CsvWorkspaceError("dataset already has five schema versions")
        previous = schemas[-1]
        existing_names = {field["name"] for field in previous["fields"]}
        if any(field["name"] in existing_names for field in additions):
            raise CsvWorkspaceError("schema version may only add new fields")
        fields = [dict(field) for field in previous["fields"]] + additions
        if len(fields) > MAX_FIELDS_PER_SCHEMA:
            raise CsvWorkspaceError("schema has too many business fields")
        for row in self._all_raw_rows(group, target):
            extra = json.loads(row[EXTRA_COLUMN])
            for field in additions:
                if field["name"] in extra:
                    _canonical_field_value(field, extra[field["name"]])
        version = target["latest_version"] + 1
        schema = self._new_schema(csv_id, version, name, description or target["description"], target["preset"], fields)
        schema["previous_schema_sha256"] = previous["_schema_sha256"]
        schema["_schema_sha256"] = _schema_checksum(schema)
        self._write_new_version(group, schema)
        event = {
            "event": "schema_version_created",
            "csv_id": csv_id,
            "version": version,
            "description": description,
            "schema_sha256": schema["_schema_sha256"],
            "created_at": _iso_now(),
            "actor_user_id": context.user_id,
            "actor_name": context.user_name,
            "source_message_id": context.message_id,
        }
        self._append_catalog(group, event)
        updated = self._logical_catalog(self._read_catalog(group))[csv_id]
        return {
            "success": True,
            "idempotent": False,
            "dataset": self._public_dataset(group, updated, schemas + [schema]),
        }

    @staticmethod
    def _new_schema(
        csv_id: str,
        version: int,
        name: str,
        description: str,
        preset: str,
        fields: list[dict[str, Any]],
    ) -> dict[str, Any]:
        schema: dict[str, Any] = {
            "schema_version": 1,
            "csv_id": csv_id,
            "version": version,
            "name": name,
            "description": description,
            "preset": preset,
            "fields": fields,
            "system_columns": list(SYSTEM_COLUMNS) + [EXTRA_COLUMN],
            "created_at": _iso_now(),
        }
        schema["_schema_sha256"] = _schema_checksum(schema)
        return schema

    def _write_new_version(self, group: Path, schema: dict[str, Any]) -> None:
        csv_id = schema["csv_id"]
        version = schema["version"]
        dataset = self._dataset_dir(group, csv_id)
        for path in (dataset, dataset / "schemas", dataset / "rows"):
            self._ensure_dir(path)
        schema_payload = _json_bytes(schema) + b"\n"
        header = list(SYSTEM_COLUMNS) + [field["name"] for field in schema["fields"]] + [EXTRA_COLUMN]
        header_payload = _csv_line(header)
        self._check_group_quota(group, extra_bytes=len(schema_payload) + len(header_payload))
        self._create_file(self._schema_path(group, csv_id, version), schema_payload)
        self._create_file(self._rows_path(group, csv_id, version), header_payload)
        stored = self._load_schema(group, csv_id, version)
        if stored["_schema_sha256"] != schema["_schema_sha256"]:
            raise CsvIntegrityError("existing immutable schema differs from the requested schema")
        self._read_rows(self._rows_path(group, csv_id, version), stored)

    def _read_rows(self, path: Path, schema: dict[str, Any]) -> list[dict[str, str]]:
        _regular_file(path, max_bytes=MAX_FILE_BYTES)
        expected = list(SYSTEM_COLUMNS) + [field["name"] for field in schema["fields"]] + [EXTRA_COLUMN]
        try:
            with _open_text_nofollow(path) as handle:
                reader = csv.reader(handle, strict=True)
                header = next(reader, None)
                if header != expected:
                    raise CsvIntegrityError("CSV header does not match its immutable schema")
                rows: list[dict[str, str]] = []
                event_ids: set[str] = set()
                for values in reader:
                    if len(values) != len(expected):
                        raise CsvIntegrityError("CSV contains a partial or malformed row")
                    row = dict(zip(expected, values))
                    if not EVENT_ID_RE.fullmatch(row["_event_id"]):
                        raise CsvIntegrityError("CSV contains an invalid event id")
                    if row["_event_id"] in event_ids:
                        raise CsvIntegrityError("CSV contains a duplicate event id")
                    event_ids.add(row["_event_id"])
                    if row["_row_sha256"] != _row_checksum(row, expected):
                        raise CsvIntegrityError("CSV row checksum validation failed")
                    _parse_timestamp(row["_created_at"])
                    try:
                        extra = json.loads(row[EXTRA_COLUMN])
                    except json.JSONDecodeError as exc:
                        raise CsvIntegrityError("CSV extra data is invalid") from exc
                    if not isinstance(extra, dict):
                        raise CsvIntegrityError("CSV extra data must be an object")
                    rows.append(row)
                return rows
        except (csv.Error, UnicodeDecodeError) as exc:
            raise CsvIntegrityError("CSV contains malformed data") from exc

    @staticmethod
    def _idempotent_event_id(
        context: SessionContext, csv_id: str, version: int, raw_row: dict[str, Any]
    ) -> str:
        canonical_input = {key: raw_row[key] for key in sorted(raw_row)}
        digest = _sha256(
            _json_bytes(
                {
                    "chat_id": context.chat_id,
                    "message_id": context.message_id,
                    "csv_id": csv_id,
                    "version": version,
                    "row": canonical_input,
                }
            )
        )
        return uuid.UUID(bytes=bytes.fromhex(digest[:32])).hex

    def _prepare_durable_row(
        self,
        context: SessionContext,
        schema: dict[str, Any],
        raw_row: dict[str, Any],
        event_id: str,
    ) -> dict[str, str]:
        field_map = {field["name"]: field for field in schema["fields"]}
        known: dict[str, str] = {}
        for name, field in field_map.items():
            known[name] = _canonical_field_value(field, raw_row.get(name))
        extra: dict[str, Any] = {}
        for key, value in raw_row.items():
            if not isinstance(key, str) or not FIELD_NAME_RE.fullmatch(key) or key.startswith("_"):
                raise CsvWorkspaceError("unknown field name is invalid")
            _reject_formula_text(key, "unknown field name")
            if key not in field_map:
                if isinstance(value, (dict, list)):
                    raise CsvWorkspaceError("unknown field values must be scalar")
                if isinstance(value, str):
                    _safe_visible_text(value, f"unknown field {key}", 4096)
                if isinstance(value, (int, float, Decimal)) and not isinstance(value, bool):
                    numeric = Decimal(str(value))
                    if not numeric.is_finite():
                        raise CsvWorkspaceError("unknown numeric field must be finite")
                    if numeric < 0:
                        raise CsvWorkspaceError("negative values require a declared decimal field")
                extra[key] = value
        row: dict[str, str] = {
            "_event_id": event_id,
            "_created_at": _iso_now(),
            "_actor_user_id": context.user_id,
            "_actor_name": context.user_name,
            "_source_message_id": context.message_id,
            "_row_sha256": "",
            **known,
            EXTRA_COLUMN: json.dumps(extra, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
        }
        header = list(SYSTEM_COLUMNS) + list(field_map) + [EXTRA_COLUMN]
        row["_row_sha256"] = _row_checksum(row, header)
        return row

    @staticmethod
    def _encode_row(schema: dict[str, Any], row: dict[str, str]) -> bytes:
        header = list(SYSTEM_COLUMNS) + [field["name"] for field in schema["fields"]] + [EXTRA_COLUMN]
        payload = _csv_line([row[column] for column in header])
        if len(payload) > MAX_ROW_BYTES:
            raise CsvWorkspaceError("encoded CSV row exceeds 16 KiB")
        return payload

    def _append_row(self, path: Path, schema: dict[str, Any], row: dict[str, str]) -> None:
        self._append_bytes(path, self._encode_row(schema, row))

    def _prepare_ledger_row(
        self,
        group: Path,
        target: dict[str, Any],
        schema: dict[str, Any],
        current_rows: list[dict[str, str]],
        raw_row: dict[str, Any],
    ) -> dict[str, Any]:
        reversal_id = raw_row.get("reverses_event_id")
        if reversal_id:
            reversal_id = _validate_event_id(reversal_id)
            forbidden = {"account", "currency", "direction", "amount", "amount_delta"} & set(raw_row)
            if forbidden:
                raise CsvWorkspaceError("ledger reversal cannot override amount, direction, account, or currency")
            all_rows = self._all_raw_rows(group, target)
            original = next((row for row in all_rows if row["_event_id"] == reversal_id), None)
            if original is None:
                raise CsvWorkspaceError("reversal target was not found in this ledger")
            if original.get("reverses_event_id"):
                raise CsvWorkspaceError("a reversal event cannot itself be reversed")
            if any(row.get("reverses_event_id") == reversal_id for row in all_rows):
                raise CsvWorkspaceError("ledger event has already been reversed")
            row = {
                "occurred_at": raw_row.get("occurred_at") or _iso_now(),
                "account": original["account"],
                "currency": original["currency"],
                "direction": "expense" if original["direction"] == "income" else "income",
                "amount": original["amount"],
                "category": raw_row.get("category") or original.get("category") or "",
                "note": raw_row.get("note") or f"Reversal of {reversal_id}",
                "reverses_event_id": reversal_id,
            }
        else:
            row = dict(raw_row)
            row.setdefault("occurred_at", _iso_now())
            row.setdefault("account", "cash")
            row.setdefault("currency", "CNY")
            if "amount_delta" in row:
                raise CsvWorkspaceError("amount_delta is generated by the ledger")
        direction = str(row.get("direction") or "")
        if direction not in {"income", "expense"}:
            raise CsvWorkspaceError("ledger direction must be income or expense")
        amount = Decimal(_canonical_decimal(row.get("amount"), "ledger amount"))
        if amount <= 0:
            raise CsvWorkspaceError("ledger amount must be positive")
        row["amount"] = _canonical_decimal(amount, "ledger amount")
        row["amount_delta"] = _canonical_decimal(amount if direction == "income" else -amount, "amount_delta")
        return row

    def _all_raw_rows(self, group: Path, target: dict[str, Any]) -> list[dict[str, str]]:
        rows: list[dict[str, str]] = []
        for version in range(1, target["latest_version"] + 1):
            schema = self._load_schema(group, target["csv_id"], version)
            rows.extend(self._read_rows(self._rows_path(group, target["csv_id"], version), schema))
        return rows

    def _append_result(
        self,
        group: Path,
        target: dict[str, Any],
        schema: dict[str, Any],
        row: dict[str, str],
        *,
        idempotent: bool,
    ) -> dict[str, Any]:
        public = self._public_row(row, schema, self._field_map([schema]))
        result: dict[str, Any] = {
            "success": True,
            "idempotent": idempotent,
            "csv_id": target["csv_id"],
            "version": schema["version"],
            "row": public,
            "checksum_verified": True,
        }
        if target["preset"] == "ledger":
            account = row["account"]
            currency = row["currency"]
            balance = Decimal("0")
            for item in self._all_raw_rows(group, target):
                if item.get("account") == account and item.get("currency") == currency:
                    balance += Decimal(item["amount_delta"])
            result["ledger_balance"] = {
                "account": account,
                "currency": currency,
                "balance": _canonical_decimal(balance, "ledger balance"),
            }
        return result

    def _logical_rows(
        self,
        group: Path,
        target: dict[str, Any],
        schemas: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        latest_fields = self._field_map(schemas)
        rows: list[dict[str, Any]] = []
        for schema in schemas:
            for row in self._read_rows(self._rows_path(group, target["csv_id"], schema["version"]), schema):
                rows.append(self._public_row(row, schema, latest_fields))
        rows.sort(key=lambda item: (item["_created_at"], item["_event_id"]))
        return rows

    @staticmethod
    def _field_map(schemas: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
        return {field["name"]: field for field in schemas[-1]["fields"]}

    def _public_row(
        self,
        row: dict[str, str],
        schema: dict[str, Any],
        latest_fields: dict[str, dict[str, Any]],
    ) -> dict[str, Any]:
        extra = json.loads(row[EXTRA_COLUMN])
        schema_fields = {field["name"]: field for field in schema["fields"]}
        visible: dict[str, Any] = {}
        for name, field in latest_fields.items():
            if name in schema_fields:
                visible[name] = _public_value(field["type"], row.get(name, ""))
            elif name in extra:
                visible[name] = _public_value(
                    field["type"],
                    _canonical_field_value({**field, "required": False}, extra.pop(name)),
                )
            else:
                visible[name] = None
        visible.update(extra)
        return {
            "_event_id": row["_event_id"],
            "_created_at": row["_created_at"],
            "_actor_name": row["_actor_name"],
            "fields": visible,
        }

    def _normalize_filters(
        self, filters: dict[str, Any], schemas: list[dict[str, Any]]
    ) -> dict[str, Any]:
        field_map = self._field_map(schemas)
        normalized: dict[str, Any] = {}
        for name, value in filters.items():
            if name == "_event_id":
                normalized[name] = _validate_event_id(value)
                continue
            field = field_map.get(name)
            if field is None:
                if not isinstance(name, str) or not FIELD_NAME_RE.fullmatch(name):
                    raise CsvWorkspaceError("filter field name is invalid")
                normalized[name] = value
                continue
            canonical = _canonical_field_value({**field, "required": False}, value)
            normalized[name] = _public_value(field["type"], canonical)
        return normalized

    @staticmethod
    def _matches(row: dict[str, Any], filters: dict[str, Any]) -> bool:
        for name, expected in filters.items():
            actual = row["_event_id"] if name == "_event_id" else row["fields"].get(name)
            if actual != expected:
                return False
        return True

    def _public_dataset(
        self,
        group: Path,
        target: dict[str, Any],
        schemas: list[dict[str, Any]],
        *,
        include_fields: bool = True,
    ) -> dict[str, Any]:
        row_count = sum(
            len(self._read_rows(self._rows_path(group, target["csv_id"], schema["version"]), schema))
            for schema in schemas
        )
        result: dict[str, Any] = {
            "csv_id": target["csv_id"],
            "name": target["name"],
            "description": target["description"],
            "preset": target["preset"],
            "created_at": target["created_at"],
            "created_by": target["created_by"],
            "latest_version": target["latest_version"],
            "version_count": len(schemas),
            "row_count": row_count,
        }
        if include_fields:
            result["versions"] = [
                {
                    "version": schema["version"],
                    "fields": [
                        {key: value for key, value in field.items() if key != "generated"}
                        for field in schema["fields"]
                    ],
                }
                for schema in schemas
            ]
        return result

    def _check_group_quota(self, group: Path, *, extra_bytes: int) -> None:
        total = 0
        for path in group.rglob("*"):
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                raise CsvIntegrityError("workspace contains a symlink")
            if stat.S_ISREG(info.st_mode):
                total += info.st_size
        if total + extra_bytes > MAX_GROUP_BYTES:
            raise CsvWorkspaceError("current group CSV workspace would exceed 100 MiB")

    def _snapshot_after_mutation(self, group: Path, context: SessionContext) -> None:
        local_date = _now().astimezone(ASIA_SHANGHAI).date().isoformat()
        scope = group.name
        snapshots = self.internal_root / "snapshots" / scope
        destination = snapshots / local_date
        for path in (self.internal_root, self.internal_root / "snapshots", snapshots):
            self._ensure_dir(path)
        temporary = snapshots / f".{local_date}.{uuid.uuid4().hex}.tmp"
        self._copy_snapshot(group, temporary)
        if destination.exists() or destination.is_symlink():
            info = destination.lstat()
            if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
                self._remove_snapshot(temporary)
                raise CsvIntegrityError("daily snapshot path is unsafe")
            backup = snapshots / f".{local_date}.{uuid.uuid4().hex}.old"
            destination.rename(backup)
            try:
                temporary.rename(destination)
            except Exception:
                backup.rename(destination)
                raise
            self._remove_snapshot(backup)
        else:
            try:
                temporary.rename(destination)
            except FileExistsError:
                self._remove_snapshot(temporary)
        self._fsync_dir(snapshots)
        self._prune_snapshots(snapshots)

    def _result_after_snapshot(
        self,
        result: dict[str, Any],
        group: Path,
        context: SessionContext,
    ) -> dict[str, Any]:
        try:
            self._snapshot_after_mutation(group, context)
        except Exception:
            result = dict(result)
            result["recovery_snapshot"] = "failed"
            result["warning"] = (
                "operation is committed and readable, but the daily recovery snapshot failed"
            )
        return result

    def _copy_snapshot(self, source: Path, destination: Path) -> None:
        self._ensure_dir(destination)
        for path in sorted(source.rglob("*")):
            relative = path.relative_to(source)
            if relative.parts and relative.parts[0] == "locks":
                continue
            info = path.lstat()
            target = destination / relative
            if stat.S_ISDIR(info.st_mode):
                self._ensure_dir(target)
            elif stat.S_ISREG(info.st_mode):
                self._ensure_dir(target.parent)
                read_flags = os.O_RDONLY
                if hasattr(os, "O_NOFOLLOW"):
                    read_flags |= os.O_NOFOLLOW
                source_fd = os.open(path, read_flags)
                with os.fdopen(source_fd, "rb") as src:
                    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
                    if hasattr(os, "O_NOFOLLOW"):
                        flags |= os.O_NOFOLLOW
                    fd = os.open(target, flags, 0o600)
                    try:
                        while True:
                            chunk = src.read(64 * 1024)
                            if not chunk:
                                break
                            _write_all(fd, chunk)
                        os.fsync(fd)
                    finally:
                        os.close(fd)
            else:
                raise CsvIntegrityError("workspace contains an unsupported snapshot source")

    def _prune_snapshots(self, snapshots: Path) -> None:
        dated = []
        for path in snapshots.iterdir():
            if not DATE_DIR_RE.fullmatch(path.name):
                continue
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
                raise CsvIntegrityError("dated snapshot path is unsafe")
            dated.append(path)
        dated.sort(key=lambda path: path.name, reverse=True)
        for path in dated[SNAPSHOT_RETENTION_DAYS:]:
            self._remove_snapshot(path)

    @staticmethod
    def _remove_snapshot(path: Path) -> None:
        info = path.lstat()
        if path.is_symlink() or not stat.S_ISDIR(info.st_mode):
            raise CsvIntegrityError("snapshot cleanup target is unsafe")
        shutil.rmtree(path)


_DEFAULT_WORKSPACE: CsvWorkspace | None = None


def _workspace() -> CsvWorkspace:
    global _DEFAULT_WORKSPACE
    if _DEFAULT_WORKSPACE is None:
        _DEFAULT_WORKSPACE = CsvWorkspace()
    return _DEFAULT_WORKSPACE


def _handle(operation: str, args: dict[str, Any]) -> str:
    try:
        if not isinstance(args, dict):
            raise CsvWorkspaceError("tool arguments must be an object")
        context = SessionContext.current()
        if operation == "list":
            _reject_unknown_args(args, {"csv_id"})
            result = _workspace().list(context, str(args.get("csv_id") or ""))
        elif operation == "create":
            _reject_unknown_args(args, {"name", "description", "preset", "fields", "version_of"})
            result = _workspace().create(context, args)
        elif operation == "append":
            _reject_unknown_args(args, {"csv_id", "row"})
            result = _workspace().append(context, args.get("csv_id"), args.get("row"))
        elif operation == "query":
            _reject_unknown_args(args, {"csv_id", "filters", "offset", "limit", "count", "sum_field"})
            result = _workspace().query(context, args)
        else:
            raise CsvWorkspaceError("unsupported Erhua CSV operation")
        return _json_result(result)
    except CsvWorkspaceError as exc:
        return _json_result({"success": False, "error": str(exc)})
    except Exception:
        return _json_result({"success": False, "error": "Erhua CSV operation failed safely"})


def _reject_unknown_args(args: dict[str, Any], allowed: set[str]) -> None:
    if set(args) - allowed:
        raise CsvWorkspaceError("tool arguments contain unsupported properties")


def handle_qintopia_erhua_csv_list(args: dict[str, Any], **_: Any) -> str:
    return _handle("list", args)


def handle_qintopia_erhua_csv_create(args: dict[str, Any], **_: Any) -> str:
    return _handle("create", args)


def handle_qintopia_erhua_csv_append(args: dict[str, Any], **_: Any) -> str:
    return _handle("append", args)


def handle_qintopia_erhua_csv_query(args: dict[str, Any], **_: Any) -> str:
    return _handle("query", args)


def check_erhua_csv_requirements() -> bool:
    """Registration is always available; each call enforces trusted group context."""

    return True
