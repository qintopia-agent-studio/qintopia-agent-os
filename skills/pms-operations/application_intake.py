"""Host-only application readback; callbacks are hints, never identity or approval.

Only explicit loopback fixtures are supported. No production Feishu credential,
arbitrary URL, model tool, attachment download or external write is accepted here.
"""
from __future__ import annotations

import hashlib
import http.client
import importlib.util
import ipaddress
import json
import os
import re
from pathlib import Path
from urllib.parse import quote, urlsplit


IDENTITY_FIELDS = ("name", "nickname", "phone")
CONTENT_FIELDS = ("arrival", "nights", "room_type", "occupation", "interests")
FIELDS = (*IDENTITY_FIELDS, *CONTENT_FIELDS, "consent", "status")
MAX_BYTES = 128 * 1024


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


def projection(raw, record, config):
    require(raw.get("code") == 0)
    row = raw.get("data", {}).get("record", {})
    require(row.get("record_id") == record and isinstance(row.get("fields"), dict))
    mapping = config["fields"]
    require(isinstance(mapping, dict) and set(mapping) <= set(FIELDS)
            and set(IDENTITY_FIELDS) <= set(mapping) and "consent" in mapping
            and all(isinstance(v, str) and 0 < len(v) <= 100 for v in mapping.values())
            and len(set(mapping.values())) == len(mapping))
    values = {key: cell(row["fields"].get(field)) for key, field in mapping.items()}
    # Missing/unknown consent never grants display. Withdrawal requires a real mapped field.
    require(type(config.get("consent_value")) in (str, bool))
    consent = (type(values["consent"]) is type(config["consent_value"])
               and values["consent"] == config["consent_value"])
    valid = True
    if "status" in mapping:
        require(isinstance(config.get("withdrawn_value"), str)
                and bool(config["withdrawn_value"]))
        valid = values["status"] != config["withdrawn_value"]
    identity = {key: values[key] for key in IDENTITY_FIELDS}
    source_version = row.get("last_modified_time")
    require(source_version is None or type(source_version) is int and source_version >= 0)
    return {"identity_hash": fingerprint(identity),
            "field_hash": fingerprint({"fields": values, "valid": valid, "consent_active": consent}),
            "valid": valid, "consent_active": consent,
            "source_version": None if source_version is None else str(source_version)}


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

    def read(self, record):
        record_ref(record)
        config = self.config
        path = ("/open-apis/bitable/v1/apps/" + quote(config["base_token"], safe="")
                + "/tables/" + quote(config["table_id"], safe="") + "/records/" + record)
        connection = http.client.HTTPConnection(self.host, self.port, timeout=10)
        try:
            connection.request("GET", path, headers={"Authorization": "Bearer " + self.token})
            response = connection.getresponse()
            raw = response.read(MAX_BYTES + 1)
            # 404/403/redirect are failed observations, never a withdrawn application.
            require(response.status == 200 and len(raw) <= MAX_BYTES)
            return projection(decode(raw), record, config)
        except Exception:
            raise ValueError("application_readback_unavailable") from None
        finally:
            connection.close()


def synchronize(record, client, host_call):
    record_ref(record)
    context = host_call({"action": "open", "record": record})
    # Read only after claiming the current observation. A CAS conflict needs a fresh GET.
    observed = client.read(record)
    saved = host_call({"action": "save", "record": record,
                       "read_token": context["read_token"], "observation": observed})
    welcome = host_call({"action": "reconcile_welcome", "record": record})
    return {**saved, "welcome": welcome}


def readback_one(resource_alias, record):
    require(os.environ.get("QINTOPIA_APPLICATION_LOCAL_ENABLE") == "1")
    config = decode(Path(os.environ["QINTOPIA_APPLICATION_LOCAL_CONFIG"]).read_bytes())
    require(config.get("resource_alias") == resource_alias)
    client = LocalApplicationClient(config, os.environ["QINTOPIA_APPLICATION_LOCAL_API_TOKEN"],
                                    enabled=True)
    spec = importlib.util.spec_from_file_location("qintopia_application_plugin",
                                                  Path(__file__).with_name("__init__.py"))
    plugin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin)
    require(plugin.enabled())

    def host_call(arguments):
        response = plugin.transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_application_intake", "arguments": {
                **arguments, "resource_alias": resource_alias},
            "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"],
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""}}, host=True)
        require(response.get("ok") is True)
        return response["result"]

    return synchronize(record, client, host_call)
