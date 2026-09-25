"""Bounded Green PMS transport. Persistence and human authority belong to the broker.

No retry, redirects, proxy inheritance, arbitrary endpoint, or response-body logging.
Production transport is fixed-origin HTTPS; plugin activation remains separately gated.
"""
from __future__ import annotations

import http.client
import ipaddress
import json
import re
import ssl
from urllib.parse import urlencode, urlsplit, quote

PRODUCTION_ORIGIN = "https://pms.qintopia.cn"
MAX_BYTES = 256 * 1024
COMMANDS = frozenset({"CREATE_ORDER", "RECORD_COLLECTION", "CHECK_IN", "CHECK_OUT",
    "RESCHEDULE_STAY", "EXTEND_STAY", "SHORTEN_STAY", "MOVE_UNIT", "CANCEL_ORDER"})
READS = {
    "availability": ("/properties/{property}/availability", {"arrivalDate", "departureDate", "unitKind", "excludeOrderId"}),
    "room_status": ("/properties/{property}/room-status", {"arrivalDate", "departureDate", "page", "pageSize", "search", "roomType", "salesMode", "status", "minCapacity", "unitKind"}),
    "orders": ("/orders", {"status", "query", "pageSize", "beforeId", "workDate", "funds"}),
    "order": ("/orders/{resource}", set()),
    "members": ("/members", {"query", "pageSize", "beforeId", "memberId", "hasContract"}),
    "member": ("/members/{resource}", set()),
    "payments": ("/external-payments", {"kind", "recommended", "amountMinor", "query", "status", "begin", "end", "beforeId", "billId", "limit"}),
    "payment_events": ("/external-payment-events", {"cursor", "limit"}),
    "reference_catalog": ("/properties/{property}/reference-catalog", set()),
}


class PmsError(Exception):
    def __init__(self, code: str, *, outcome_unknown: bool = False):
        super().__init__(code)
        self.code = code
        self.outcome_unknown = outcome_unknown


def require(condition, code="invalid_arguments"):
    if not condition:
        raise PmsError(code)


def identifier(value):
    require(isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]{1,160}", value))
    return value


def decode(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "invalid_pms_response")
            result[key] = value
        return result
    try:
        value = json.loads(raw, object_pairs_hook=unique,
                          parse_constant=lambda _: (_ for _ in ()).throw(ValueError()))
        require(isinstance(value, dict), "invalid_pms_response")
        return value
    except (ValueError, UnicodeError, RecursionError) as exc:
        raise PmsError("invalid_pms_response") from None


def redact(value):
    """Conservative disclosure ceiling; callers may project a smaller purpose-specific view."""
    private = {"identitycardnumber", "phone", "wechat", "token", "tokensecret", "authorization",
               "password", "secret", "email", "raw", "rawpayload", "evidencenote"}
    if isinstance(value, dict):
        return {k: redact(v) for k, v in value.items()
                if re.sub(r"[^a-z]", "", k.lower()) not in private}
    if isinstance(value, list):
        return [redact(v) for v in value]
    return value


class Client:
    def __init__(self, base_url: str, token: str, *, local_enabled: bool = False, production_enabled: bool = False, timeout=30):
        require(type(local_enabled) is bool and type(production_enabled) is bool
                and local_enabled != production_enabled, "pms_configuration_required")
        try:
            url = urlsplit(base_url)
            valid = (local_enabled and url.scheme == "http" and url.hostname is not None
                     and ipaddress.ip_address(url.hostname).is_loopback and url.port is not None
                     and not url.username and not url.password and not url.query and not url.fragment
                     and url.path in {"", "/"})
            if production_enabled:
                valid = base_url in {PRODUCTION_ORIGIN, PRODUCTION_ORIGIN + "/"}
        except (ValueError, TypeError):
            valid = False
        require(valid, "pms_configuration_required")
        require(isinstance(token, str) and 16 <= len(token) <= 512
                and all(33 <= ord(c) <= 126 for c in token), "pms_credential_unavailable")
        require(isinstance(timeout, (int, float)) and 0 < timeout <= 30)
        self._host, self._port, self._token, self._timeout = url.hostname, url.port, token, timeout
        self._production = production_enabled

    def __repr__(self):
        return "<PmsClient production>" if self._production else "<PmsClient local-only>"

    def _request(self, method, path, *, payload=None, key=None, correlation=None):
        require(path.startswith("/api/v1/") and "\r" not in path and "\n" not in path)
        headers = {"Authorization": "Bearer " + self._token, "Accept": "application/json"}
        body = None
        if method == "POST":
            identifier(key)
            identifier(correlation)
            try:
                body = json.dumps(payload, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode()
            except (ValueError, TypeError, RecursionError):
                raise PmsError("invalid_arguments") from None
            require(len(body) <= MAX_BYTES)
            headers.update({"Content-Type": "application/json", "Idempotency-Key": key,
                            "X-Correlation-ID": correlation})
        connection = (http.client.HTTPSConnection(
            self._host, self._port, timeout=self._timeout, context=ssl.create_default_context())
            if self._production else http.client.HTTPConnection(self._host, self._port, timeout=self._timeout))
        attempted = False
        try:
            attempted = True
            connection.request(method, path, body=body, headers=headers)
            response = connection.getresponse()
            raw = response.read(MAX_BYTES + 1)
            require(len(raw) <= MAX_BYTES, "invalid_pms_response")
            # Never follow a redirect, including with a Bearer header.
            if method == "POST" and path == "/api/v1/command-previews" and response.status in {400, 403, 404, 409, 422}:
                error = decode(raw)
                if isinstance(error.get("code"), str) and re.fullmatch(r"[A-Z_]{1,80}", error["code"]) and error.get("retryable") is False:
                    # This is a rejected preparation, not a PMS business receipt.
                    raise PmsError("pms_preview_rejected")
            if response.status not in {200, 409}:
                raise PmsError("pms_request_rejected", outcome_unknown=method == "POST")
            result = decode(raw)
            if response.status == 409 and not self.is_receipt(result):
                raise PmsError("pms_request_rejected", outcome_unknown=method == "POST")
            return result
        except PmsError as exc:
            if attempted and method == "POST" and exc.code != "pms_preview_rejected":
                exc.outcome_unknown = True
            raise
        except (OSError, http.client.HTTPException, ValueError):
            raise PmsError("pms_outcome_unknown" if attempted and method == "POST" else "pms_unavailable",
                           outcome_unknown=attempted and method == "POST") from None
        finally:
            connection.close()

    @staticmethod
    def is_receipt(result):
        return (isinstance(result, dict) and result.get("executionStatus") in {"EXECUTED", "NOT_EXECUTED"}
                and type(result.get("businessCommitted")) is bool
                and result["businessCommitted"] == (result["executionStatus"] == "EXECUTED")
                and isinstance(result.get("receiptId"), str) and isinstance(result.get("commandId"), str))

    def me(self):
        value = self._request("GET", "/api/v1/me")
        require(all(isinstance(value.get(k), dict) for k in
                    ("propertyAccess", "propertyCommandGrants", "allowedActions")), "invalid_pms_response")
        return value

    def authorize(self, property_id, command=None):
        identifier(property_id)
        me = self.me()
        require(me["propertyAccess"].get(property_id) in {"READ", "WRITE"}, "pms_property_denied")
        if command and command != "CREATE_QUOTE":
            require(command in COMMANDS, "unsupported_command")
            require(me["propertyAccess"][property_id] == "WRITE"
                    and command in me["allowedActions"].get(property_id, [])
                    and command in me["propertyCommandGrants"].get(property_id, []), "pms_command_denied")
        return me

    def payment_head(self, property_id):
        self.authorize(property_id)
        return self._request("GET", "/api/v1/external-payment-events/head?" + urlencode({"propertyId": property_id}))

    def read(self, kind, property_id, *, resource=None, filters=None):
        require(kind in READS, "unsupported_query")
        self.authorize(property_id)
        template, allowed = READS[kind]
        filters = dict(filters or {})
        require(not set(filters) - allowed)
        require(all(type(v) in {str, int, bool} for v in filters.values()))
        for bound in ("limit", "pageSize"):
            if bound in filters:
                require(type(filters[bound]) is int and 1 <= filters[bound] <= 100)
        if "{resource}" in template:
            identifier(resource)
        path = "/api/v1" + template.format(property=quote(property_id), resource=quote(resource or ""))
        if "{property}" not in template and kind != "order":
            filters["propertyId"] = property_id
        path += "?" + urlencode({k: str(v).lower() if type(v) is bool else v for k, v in filters.items()}) if filters else ""
        result = self._request("GET", path)
        # The token may cover several properties; a model-selected order cannot cross the broker scope.
        if kind == "order":
            order = result.get("order", result)
            require(order.get("propertyId", order.get("property_id")) == property_id, "pms_property_denied")
        return result

    def quote(self, payload, *, key, correlation):
        self.authorize(payload.get("propertyId"), "CREATE_QUOTE")
        return self._request("POST", "/api/v1/quotes", payload=payload, key=key, correlation=correlation)

    def preview(self, command, payload, *, key, correlation):
        self.authorize(payload.get("propertyId"), command)
        return self._request("POST", "/api/v1/command-previews", payload={"commandType": command, "input": payload}, key=key, correlation=correlation)

    def confirm(self, preview, reason, *, key, correlation):
        # Only the orchestrator with a durable claim calls this method; never a raw model tool.
        self.authorize(preview.get("propertyId"), preview.get("commandType"))
        identifier(preview.get("previewId"))
        require(isinstance(preview.get("effectHash"), str) and re.fullmatch("[a-f0-9]{64}", preview["effectHash"]))
        result = self._request("POST", "/api/v1/command-previews/" + preview["previewId"] + "/confirm",
            payload={"propertyId": preview["propertyId"], "commandType": preview["commandType"],
                     "expectedEffectHash": preview["effectHash"], "confirmation": True, "reason": reason}, key=key, correlation=correlation)
        require(self.is_receipt(result), "invalid_pms_response")
        return result

    def recover(self, property_id, command, original_key, *, resolve_key=None, correlation=None):
        require(command in COMMANDS or command == "CREATE_QUOTE", "unsupported_command")
        identifier(original_key)
        self.authorize(property_id, command)
        payload = {"propertyId": property_id, "commandType": command, "idempotencyKey": original_key}
        result = self._request("GET", "/api/v1/command-results?" + urlencode(payload))
        if result.get("executionStatus") == "UNKNOWN" and resolve_key:
            require(resolve_key != original_key)
            result = self._request("POST", "/api/v1/command-results/resolve", payload=payload,
                                   key=resolve_key, correlation=correlation)
        require(result.get("executionStatus") == "UNKNOWN" or self.is_receipt(result), "invalid_pms_response")
        return result
