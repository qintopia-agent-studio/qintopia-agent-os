"""Local socket-activated, read-only interface for the registered Tuya sensors."""

import json
import os
import socket
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import sensors


class Reader:
    def __init__(self):
        path = Path(os.environ["CREDENTIALS_DIRECTORY"]) / "tuya.json"
        credentials = json.loads(path.read_text(encoding="utf-8"))
        os.environ["TUYA_CLIENT_ID"] = credentials["client_id"]
        os.environ["TUYA_CLIENT_SECRET"] = credentials["client_secret"]
        try:
            self.cloud = sensors.TuyaCloud()
        finally:
            del os.environ["TUYA_CLIENT_ID"], os.environ["TUYA_CLIENT_SECRET"]
        self.expires = 0

    def sign_in(self):
        if self.cloud.token and time.monotonic() < self.expires:
            return
        self.cloud.token = ""
        result = self.cloud.get("/v1.0/token?grant_type=1").get("result") or {}
        token = result.get("access_token")
        if not isinstance(token, str) or not token:
            raise sensors.QueryError("涂鸦未返回有效的访问令牌。")
        self.cloud.token = token
        self.expires = time.monotonic() + max(0, int(result.get("expire_time") or 0) - 60)

    def read(self, device, profiles):
        self.sign_in()
        try:
            return sensors.read_device(self.cloud, device, profiles)
        except sensors.QueryError as error:
            error_code = str(error).partition("，")[0]
            if error_code not in {"涂鸦错误码 1010", "涂鸦错误码 1011", "涂鸦错误码 1012"}:
                raise
        self.expires = 0
        self.sign_in()
        return sensors.read_device(self.cloud, device, profiles)

    def handle(self, request):
        devices, profiles = sensors.load_registry()
        if request == {"op": "list"}:
            return {"ok": True, "devices": [{"name": d["name"], "area": d["area"]} for d in devices]}
        if request == {"op": "read", "all": True}:
            chosen = devices
        elif (isinstance(request, dict) and set(request) == {"op", "target"}
              and request["op"] == "read" and isinstance(request["target"], str)
              and 0 < len(request["target"]) <= 128):
            chosen = sensors.select_devices(devices, request["target"])
        else:
            raise sensors.QueryError("支持按设备名称、区域或全部已登记设备查询。")
        if not chosen:
            raise sensors.QueryError("设备清单为空。")
        rows = []
        for device in chosen:
            row = {"name": device["name"], "area": device["area"]}
            try:
                row.update(self.read(device, profiles))
            except sensors.QueryError as error:
                row.update(ok=False, error=str(error))
            except (ValueError, KeyError, TypeError, AttributeError, OverflowError):
                row.update(ok=False, error="设备配置或涂鸦响应格式异常。")
            rows.append(row)
        return {"ok": all(row["ok"] for row in rows), "devices": rows}


def main():
    if os.getenv("LISTEN_FDS") != "1" or os.getenv("LISTEN_PID") != str(os.getpid()):
        raise SystemExit("Run through the qintopia-tuya-sensors socket unit.")
    reader = Reader()
    with socket.socket(fileno=3) as listener:
        while True:
            connection, _ = listener.accept()
            with connection:
                connection.settimeout(4)
                try:
                    with connection.makefile("rb") as stream:
                        line = stream.readline(4097)
                    if len(line) > 4096 or not line.endswith(b"\n"):
                        raise ValueError
                    response = reader.handle(json.loads(line))
                except sensors.QueryError as error:
                    response = {"ok": False, "error": str(error)}
                except (OSError, ValueError, TypeError, KeyError, AttributeError):
                    response = {"ok": False, "error": "请核对查询参数和设备清单。"}
                try:
                    connection.sendall(json.dumps(response, ensure_ascii=False, allow_nan=False).encode() + b"\n")
                except OSError:
                    pass  # A caller can leave without affecting other readers.


if __name__ == "__main__":
    main()
