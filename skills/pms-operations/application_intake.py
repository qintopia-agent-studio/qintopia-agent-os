"""Host-only application readback; callbacks are hints, never identity or approval.

Production uses a fixed Feishu HTTPS origin and a private host-owned config file.
No arbitrary URL, model tool, attachment download or external write is accepted here.
"""
from __future__ import annotations

import hashlib
import http.client
import importlib.util
import ipaddress
import json
import os
import re
import socket
import ssl
import stat
from pathlib import Path
from urllib.parse import quote, urlsplit


IDENTITY_FIELDS = ("name", "nickname", "phone")
CONTENT_FIELDS = ("arrival", "nights", "room_type", "occupation", "interests")
FIELDS = (*IDENTITY_FIELDS, *CONTENT_FIELDS, "consent", "status")
MAX_BYTES = 128 * 1024
PRIVATE_MAX_BYTES = 16 * 1024
FEISHU_HOST = "open.feishu.cn"
PRODUCTION_CONFIG_ROOT = Path("/etc/qintopia")
PRODUCTION_CONFIG_KEYS = {"resource_alias", "base_token", "table_id", "fields",
                          "consent_value", "app_id", "app_secret", "foundation_host_token"}
MODEL_ENV_SECRETS = ("FEISHU_APP_ID", "FEISHU_APP_SECRET", "LARK_APP_ID", "LARK_APP_SECRET",
                     "QINTOPIA_FOUNDATION_HOST_TOKEN")

_production_spec = importlib.util.spec_from_file_location(
    "qintopia_application_production", Path(__file__).with_name("production.py"))
production = importlib.util.module_from_spec(_production_spec)
_production_spec.loader.exec_module(production)


def require(condition):
    if not condition:
        raise ValueError("application_readback_unavailable")


def fingerprint(value):
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True,
                                    allow_nan=False, separators=(",", ":")).encode()).hexdigest()


def record_ref(value):
    require(isinstance(value, str) and re.fullmatch(r"rec[A-Za-z0-9]{1,100}", value))
    return value


def decode(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result)
            result[key] = value
        return result
    value = json.loads(raw, object_pairs_hook=unique,
                       parse_constant=lambda _: require(False))
    require(isinstance(value, dict))
    return value


def cell(value):
    # Only bounded scalar/rich-text cells. Attachment and contact objects are not accepted.
    if value is None or type(value) in (bool, int):
        return value
    if isinstance(value, str):
        require(len(value) <= 2048)
        return value.strip()
    if isinstance(value, list):
        require(len(value) <= 20 and all(isinstance(v, dict) and set(v) <= {"text", "type"}
                                       and isinstance(v.get("text"), str) for v in value))
        return cell("".join(v["text"] for v in value))
    require(False)


def validate_fields(config):
    mapping = config.get("fields")
    require(isinstance(mapping, dict) and set(mapping) <= set(FIELDS)
            and set(IDENTITY_FIELDS) <= set(mapping) and "consent" in mapping
            and all(isinstance(v, str) and 0 < len(v) <= 100 for v in mapping.values())
            and len(set(mapping.values())) == len(mapping))
    require(type(config.get("consent_value")) in (str, bool))
    if "status" in mapping:
        require(isinstance(config.get("withdrawn_value"), str)
                and bool(config["withdrawn_value"]))
    return mapping


def projection(raw, record, config, *, include_fields=False):
    require(type(raw.get("code")) is int and raw["code"] == 0)
    row = raw.get("data", {}).get("record", {})
    require(row.get("record_id") == record and isinstance(row.get("fields"), dict))
    mapping = validate_fields(config)
    values = {key: cell(row["fields"].get(field)) for key, field in mapping.items()}
    # Missing/unknown consent never grants display. Withdrawal requires a real mapped field.
    consent = (type(values["consent"]) is type(config["consent_value"])
               and values["consent"] == config["consent_value"])
    valid = True
    if "status" in mapping:
        valid = values["status"] != config["withdrawn_value"]
    identity = {key: values[key] for key in IDENTITY_FIELDS}
    source_version = row.get("last_modified_time")
    require(source_version is None or type(source_version) is int and source_version >= 0)
    result = {"identity_hash": fingerprint(identity),
            "field_hash": fingerprint({"fields": values, "valid": valid, "consent_active": consent}),
            "valid": valid, "consent_active": consent,
            "source_version": None if source_version is None else str(source_version)}
    if include_fields:
        result["fields"] = values
    return result


class LocalApplicationClient:
    def __init__(self, config, token, *, enabled=False):
        try:
            url = urlsplit(config["base_url"])
            valid = (enabled and url.scheme == "http" and url.hostname is not None
                     and ipaddress.ip_address(url.hostname).is_loopback and url.port is not None
                     and not url.username and not url.password and not url.query and not url.fragment
                     and url.path in ("", "/"))
        except (ValueError, TypeError, KeyError):
            valid = False
        require(valid)
        require(isinstance(token, str) and 16 <= len(token) <= 512
                and all(33 <= ord(c) <= 126 for c in token))
        for key in ("base_token", "table_id"):
            require(isinstance(config.get(key), str)
                    and re.fullmatch(r"[A-Za-z0-9_-]{1,160}", config[key]))
        self.config = config
        self.host, self.port, self.token = url.hostname, url.port, token

    def __repr__(self):
        return "<ApplicationClient local-only>"

    def read(self, record, *, include_fields=False):
        record_ref(record)
        config = self.config
        path = record_path(config, record)
        connection = http.client.HTTPConnection(self.host, self.port, timeout=10)
        try:
            connection.request("GET", path, headers={"Authorization": "Bearer " + self.token})
            response = connection.getresponse()
            raw = response.read(MAX_BYTES + 1)
            # 404/403/redirect are failed observations, never a withdrawn application.
            require(response.status == 200 and len(raw) <= MAX_BYTES)
            return projection(decode(raw), record, config, include_fields=include_fields)
        except Exception:
            raise ValueError("application_readback_unavailable") from None
        finally:
            connection.close()


def record_path(config, record):
    record_ref(record)
    return ("/open-apis/bitable/v1/apps/" + quote(config["base_token"], safe="")
            + "/tables/" + quote(config["table_id"], safe="") + "/records/" + record)


def load_production_config(path, *, trusted_root=PRODUCTION_CONFIG_ROOT):
    fd = None
    try:
        candidate = Path(path)
        require(candidate.is_absolute() and candidate != trusted_root
                and candidate.is_relative_to(trusted_root) and ".." not in candidate.parts
                and not any(key in os.environ for key in MODEL_ENV_SECRETS))
        fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
        for index, part in enumerate(candidate.parts[1:]):
            final = index == len(candidate.parts) - 2
            flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
            if not final:
                flags |= os.O_DIRECTORY
            child = os.open(part, flags, dir_fd=fd)
            os.close(fd)
            fd = child
            info = os.fstat(fd)
            if final:
                require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                        and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1
                        and info.st_size <= PRIVATE_MAX_BYTES)
            else:
                require(info.st_uid in (0, os.getuid()) and not info.st_mode & 0o022)
        config = decode(os.read(fd, PRIVATE_MAX_BYTES + 1))
        require(set(config) in (PRODUCTION_CONFIG_KEYS, PRODUCTION_CONFIG_KEYS | {"withdrawn_value"}))
        require(isinstance(config["resource_alias"], str)
                and re.fullmatch(r"[a-z][a-z0-9_-]{0,79}", config["resource_alias"]))
        for key in ("base_token", "table_id"):
            require(isinstance(config[key], str) and re.fullmatch(r"[A-Za-z0-9_-]{1,160}", config[key]))
        validate_fields(config)
        require(isinstance(config["app_id"], str) and re.fullmatch(r"[A-Za-z0-9_-]{1,160}", config["app_id"]))
        for key, minimum, maximum in (("app_secret", 16, 512), ("foundation_host_token", 32, 256)):
            value = config[key]
            require(isinstance(value, str) and minimum <= len(value) <= maximum
                    and value.isascii() and all(33 <= ord(char) <= 126 for char in value))
        require(config["app_secret"] != config["foundation_host_token"])
        return config
    except Exception:
        raise ValueError("application_readback_unavailable") from None
    finally:
        if fd is not None:
            os.close(fd)


class ProductionApplicationClient:
    def __init__(self, config):
        self.config = config

    def __repr__(self):
        return "<ApplicationClient production>"

    def _request(self, method, path, *, body=None, token=None):
        headers = {"Accept": "application/json"}
        if token is not None:
            headers["Authorization"] = "Bearer " + token
        if body is not None:
            headers["Content-Type"] = "application/json"
        connection = http.client.HTTPSConnection(
            FEISHU_HOST, 443, timeout=10, context=ssl.create_default_context())
        try:
            connection.request(method, path, body=body, headers=headers)
            response = connection.getresponse()
            raw = response.read(MAX_BYTES + 1)
            require(response.status == 200 and len(raw) <= MAX_BYTES)
            return decode(raw)
        except Exception:
            raise ValueError("application_readback_unavailable") from None
        finally:
            connection.close()

    def read(self, record, *, include_fields=False):
        try:
            record_ref(record)
            body = json.dumps({"app_id": self.config["app_id"],
                               "app_secret": self.config["app_secret"]}, separators=(",", ":")).encode()
            require(len(body) <= 2048)
            issued = self._request("POST", "/open-apis/auth/v3/tenant_access_token/internal", body=body)
            token = issued.get("tenant_access_token")
            require(type(issued.get("code")) is int and issued["code"] == 0
                    and isinstance(token, str) and 16 <= len(token) <= 512
                    and token.isascii() and all(33 <= ord(char) <= 126 for char in token))
            source = self._request("GET", record_path(self.config, record), token=token)
            return projection(source, record, self.config, include_fields=include_fields)
        except Exception:
            raise ValueError("application_readback_unavailable") from None


def application_mode():
    local = [os.environ.get(key) == "1" for key in
             ("QINTOPIA_APPLICATION_LOCAL_ENABLE", "QINTOPIA_FOUNDATION_LOCAL_ENABLE")]
    production = [os.environ.get(key) == "1" for key in
                  ("QINTOPIA_APPLICATION_PRODUCTION_ENABLE", "QINTOPIA_FOUNDATION_PRODUCTION_ENABLE")]
    if any(local) and any(production):
        return "disabled"
    if all(local):
        return "local"
    if all(production):
        return "production"
    return "disabled"


def production_transport(config):
    path = Path(os.environ.get("QINTOPIA_FOUNDATION_SOCKET", ""))
    require(path.is_absolute() and not path.is_symlink())
    info = path.stat()
    require(stat.S_ISSOCK(info.st_mode) and info.st_uid == os.getuid())
    token = config["foundation_host_token"]

    def transport(request):
        raw = json.dumps({**request, "token": token}, ensure_ascii=False, allow_nan=False).encode() + b"\n"
        require(len(raw) <= 256 * 1024)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(10)
            connection.connect(str(path))
            connection.sendall(raw)
            received = bytearray()
            while not received.endswith(b"\n"):
                chunk = connection.recv(min(4096, 256 * 1024 + 1 - len(received)))
                require(chunk and len(received) + len(chunk) <= 256 * 1024)
                received.extend(chunk)
        return decode(received)

    return transport


def candidate_fields(values):
    require(isinstance(values, dict) and set(values) <= set(FIELDS)
            and {*IDENTITY_FIELDS, "consent"} <= set(values))
    for key, value in values.items():
        require(value is None or type(value) in (bool, int) or isinstance(value, str)
                and "\x00" not in value and len(value) <=
                (120 if key in ("name", "nickname") else 40 if key in ("phone", "arrival") else 2048))
    return values


def synchronize(record, client, host_call, *, project_candidates=None):
    record_ref(record)
    context = host_call({"action": "open", "record": record})
    # Read only after claiming the current observation. A CAS conflict needs a fresh GET.
    observed = (client.read(record, include_fields=True) if project_candidates else client.read(record))
    fields = observed.pop("fields", None)
    saved = host_call({"action": "save", "record": record,
                       "read_token": context["read_token"], "observation": observed})
    projection_result = None
    if project_candidates:
        projection_result = {"stored": False, "identity_confirmed": False,
                             "status": "not_eligible"}
        if observed["valid"] and observed["consent_active"]:
            try:
                result = project_candidates(saved["application"], candidate_fields(fields))
                require(result == {"stored": True, "identity_confirmed": False})
                projection_result = result
            except Exception:
                # The source save and both original work items already committed.
                # A rejection/lost acknowledgement is not a failed source save.
                projection_result = {"stored": None, "identity_confirmed": False,
                                     "status": "projection_unconfirmed"}
    try:
        welcome = host_call({"action": "reconcile_welcome", "record": record})
    except Exception:
        if not project_candidates:
            raise
        welcome = {"status": "handoff_unconfirmed"}
    return {**saved, "welcome": welcome,
            **({"candidate_projection": projection_result} if project_candidates else {})}


def readback_one(resource_alias, record):
    mode = application_mode()
    if mode == "local":
        config = decode(Path(os.environ["QINTOPIA_APPLICATION_LOCAL_CONFIG"]).read_bytes())
        client = LocalApplicationClient(config, os.environ["QINTOPIA_APPLICATION_LOCAL_API_TOKEN"],
                                        enabled=True)
        spec = importlib.util.spec_from_file_location("qintopia_application_plugin",
                                                      Path(__file__).with_name("__init__.py"))
        plugin = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(plugin)
        require(plugin.enabled())

        def transport(request):
            return plugin.transport(request, host=True)
    elif mode == "production":
        config_path = os.environ["QINTOPIA_APPLICATION_PRODUCTION_CONFIG"]
        production.require(private_paths=(config_path,))
        config = load_production_config(config_path)
        client = ProductionApplicationClient(config)
        transport = production_transport(config)
    else:
        require(False)
    require(config.get("resource_alias") == resource_alias
            and os.environ.get("QINTOPIA_APPLICATION_RESOURCE_ALIAS") == resource_alias)

    def host_call(arguments):
        response = transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_application_intake", "arguments": {
                **arguments, "resource_alias": resource_alias},
            "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"],
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""}})
        require(response.get("ok") is True)
        return response["result"]

    def project_candidates(application, fields):
        response = transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "welcome_source_projection",
            "arguments": {"binding": os.environ["QINTOPIA_APPLICATION_BINDING"],
                          "application": application, "fields": fields},
            "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"],
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""}})
        require(response.get("ok") is True)
        return response["result"]

    return synchronize(record, client, host_call, project_candidates=project_candidates)
