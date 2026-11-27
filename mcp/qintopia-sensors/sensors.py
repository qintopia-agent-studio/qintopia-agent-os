#!/usr/bin/env python3
"""Read the latest reported values from registered Tuya devices in China."""

import argparse
import hashlib
import hmac
import json
import os
import re
import sys
import time
from datetime import datetime, timedelta, timezone
from decimal import Decimal, InvalidOperation
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import HTTPRedirectHandler, Request, build_opener


ENDPOINT = "https://openapi.tuyacn.com"
EMPTY_HASH = hashlib.sha256(b"").hexdigest()
BEIJING = timezone(timedelta(hours=8))
DEFAULT_REGISTRY = Path("/etc/qintopia-tuya-sensors/devices.json")


class QueryError(Exception):
    pass


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # Send signed headers only to the configured Tuya endpoint.
        return None


class TuyaCloud:
    def __init__(self):
        self.client_id = os.environ.get("TUYA_CLIENT_ID", "").strip()
        self.secret = os.environ.get("TUYA_CLIENT_SECRET", "").strip()
        if not self.client_id or not self.secret:
            raise QueryError("请在运行环境的密钥配置中设置 TUYA_CLIENT_ID 和 TUYA_CLIENT_SECRET。")
        self.http = build_opener(NoRedirect())
        self.token = ""

    def get(self, path):
        timestamp = str(time.time_ns() // 1_000_000)
        canonical = f"GET\n{EMPTY_HASH}\n\n{path}"
        message = self.client_id + self.token + timestamp + canonical
        signature = hmac.new(self.secret.encode(), message.encode(), hashlib.sha256)
        headers = {"client_id": self.client_id, "t": timestamp,
                   "sign_method": "HMAC-SHA256", "sign": signature.hexdigest().upper()}
        if self.token:
            headers["access_token"] = self.token
        request = Request(ENDPOINT + path, headers=headers, method="GET")
        try:
            with self.http.open(request, timeout=15) as response:
                data = json.load(response)
        except HTTPError as error:
            raise QueryError(f"涂鸦 HTTP {error.code}。") from None
        except (URLError, TimeoutError, OSError):
            raise QueryError("连接涂鸦中国数据中心失败，请检查网络后重试。") from None
        except (ValueError, UnicodeError):
            raise QueryError("涂鸦响应不是有效的 JSON。") from None
        if not isinstance(data, dict):
            raise QueryError("涂鸦响应格式异常。")
        if data.get("success") is not True:
            # Report only the error identifiers, never the raw body or request headers.
            raise QueryError(f"涂鸦错误码 {data.get('code', '未知')}，tid {data.get('tid', '未知')}。")
        return data

    def connect(self):
        data = self.get("/v1.0/token?grant_type=1")
        token = (data.get("result") or {}).get("access_token")
        if not isinstance(token, str) or not token:
            raise QueryError("涂鸦没有返回访问令牌。")
        self.token = token


def lookup_key(label):
    label = label.strip()
    floor, separator, room = label.partition("楼")
    digits = "零一二三四五六七八九"
    if separator and len(floor) == 1 and floor in digits:
        return f"{digits.index(floor)}楼{room}"
    return label


def load_registry():
    path = Path(os.environ.get("TUYA_DEVICES_FILE") or DEFAULT_REGISTRY)
    registry = json.loads(path.read_text(encoding="utf-8"))
    devices, profiles = registry["devices"], registry["profiles"]
    if not isinstance(devices, list) or not isinstance(profiles, dict):
        raise ValueError("设备清单需要 devices 数组和 profiles 对象。")
    names, ids = set(), set()
    for device in devices:
        name, area, device_id = device["name"], device["area"], device["device_id"]
        if not name.strip() or not area.strip() or not re.fullmatch(r"[A-Za-z0-9_-]+", device_id):
            raise ValueError("请核对设备名称、区域和设备 ID。")
        if lookup_key(name) in names or device_id in ids:
            raise ValueError(f"设备清单里有重复的名称或 ID，涉及「{name}」。")
        names.add(lookup_key(name))
        ids.add(device_id)
    return devices, profiles


def select_devices(devices, target):
    if target is None:
        return devices
    key = lookup_key(target)
    by_name = [device for device in devices if lookup_key(device["name"]) == key]
    selected = by_name or [device for device in devices if lookup_key(device["area"]) == key]
    if not selected:
        raise QueryError(f"清单中找不到「{target}」，请先运行 list 查看已登记的名称和区域。")
    return selected


def local_time(timestamp):
    if not isinstance(timestamp, (int, float)) or timestamp <= 0:
        return None
    return datetime.fromtimestamp(timestamp / 1000, BEIJING).isoformat(timespec="seconds")


def format_reading(code, definition, properties):
    point = properties.get(code, {})
    result = {"code": code, "label": definition["label"],
              "reported_at": local_time(point.get("time"))}
    value = point.get("value")
    if value is None:
        return dict(result, status="not_reported")
    unit = definition.get("unit", "")
    if "unit_from" in definition:
        selector = definition["unit_from"]
        mode = properties.get(selector["code"], {}).get("value")
        unit = selector["values"].get(str(mode))
        if unit is None:
            return dict(result, status="unit_unconfirmed", unit_setting=mode, raw_value=value)
    if "scale" in definition:
        scale = definition["scale"]
        if type(scale) is not int or not 0 <= scale <= 10:
            raise ValueError(f"请核对 {code} 的 scale。")
        try:
            numeric = Decimal(str(value)).scaleb(-scale)
            if not numeric.is_finite():
                raise InvalidOperation
            value = int(numeric) if numeric == numeric.to_integral_value() else float(numeric)
        except InvalidOperation:
            return dict(result, status="invalid_value")
    status = "ok" if result["reported_at"] else "time_unknown"
    return dict(result, status=status, value=value, unit=unit)


def read_device(cloud, device, profiles):
    definitions = profiles.get(device.get("profile"))
    if not isinstance(definitions, dict) or not definitions:
        raise QueryError("请用 describe 核对设备的物模型，并为该设备配置 profile。")
    codes = set(definitions)
    for definition in definitions.values():
        if "unit_from" in definition:
            codes.add(definition["unit_from"]["code"])
    # Tuya accepts a comma-delimited codes parameter. Sign the exact path we send.
    selected = quote(",".join(sorted(codes)), safe=",")
    base = f"/v2.0/cloud/thing/{device['device_id']}"
    data = cloud.get(f"{base}/shadow/properties?codes={selected}")
    items = (data.get("result") or {}).get("properties")
    if not isinstance(items, list):
        raise QueryError("涂鸦响应中缺少设备属性列表。")
    points = {item["code"]: item for item in items}
    readings = [format_reading(code, definition, points) for code, definition in definitions.items()]
    return {"ok": all(item["status"] == "ok" for item in readings),
            "queried_at": local_time(data.get("t")), "readings": readings}


def describe_device(cloud, device):
    data = cloud.get(f"/v2.0/cloud/thing/{device['device_id']}/model")
    model = (data.get("result") or {}).get("model")
    if isinstance(model, str):
        model = json.loads(model)
    if not isinstance(model, dict) or not isinstance(model.get("services"), list):
        raise QueryError("涂鸦响应中缺少设备物模型。")
    properties = []
    for service in model["services"]:
        for item in service.get("properties", []):
            spec = item.get("typeSpec", {})
            properties.append({"code": item.get("code"), "name": item.get("name"),
                               "type": spec.get("type"), "scale": spec.get("scale"),
                               "unit": spec.get("unit"), "range": spec.get("range")})
    return {"ok": True, "properties": properties}


def run(args):
    devices, profiles = load_registry()
    if args.command == "list":
        return {"ok": True, "devices": [{"name": d["name"], "area": d["area"]} for d in devices]}
    selected = select_devices(devices, args.target)
    if not selected:
        raise QueryError("设备清单为空。")
    cloud = TuyaCloud()
    cloud.connect()
    results = []
    for device in selected:
        row = {"name": device["name"], "area": device["area"]}
        try:
            if args.command == "describe":
                row.update(describe_device(cloud, device))
            else:
                row.update(read_device(cloud, device, profiles))
        except (QueryError, KeyError, ValueError, TypeError, AttributeError, OverflowError) as error:
            row.update(ok=False, error=str(error))
        results.append(row)
    return {"ok": all(row["ok"] for row in results), "devices": results}


def main():
    parser = argparse.ArgumentParser(description="按设备名称或区域读取秦托邦涂鸦设备的云端上报值。")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list", help="列出已经登记的设备和区域")
    read = commands.add_parser("read", help="读取最新上报值")
    targets = read.add_mutually_exclusive_group(required=True)
    targets.add_argument("target", nargs="?", help="设备名称或区域")
    targets.add_argument("--all", action="store_true", help="读取清单中的全部设备")
    describe = commands.add_parser("describe", help="查看已登记设备的云端属性定义")
    describe.add_argument("target", help="设备名称或区域")
    args = parser.parse_args()
    try:
        result = run(args)
    except (QueryError, OSError, KeyError, ValueError, TypeError, AttributeError) as error:
        result = {"ok": False, "error": str(error)}
    print(json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
