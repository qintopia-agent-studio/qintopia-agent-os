#!/usr/bin/python3 -I
"""Query the shared, local Tuya readings service."""

import argparse
import json
import socket
import sys


def main():
    cli = argparse.ArgumentParser(description="秦托邦各区域的最新设备读数")
    commands = cli.add_subparsers(dest="op", required=True)
    commands.add_parser("list", help="显示区域与设备")
    read = commands.add_parser("read", help="查询最近上报值")
    group = read.add_mutually_exclusive_group(required=True)
    group.add_argument("target", nargs="?", help="区域或设备名称")
    group.add_argument("--all", action="store_true", help="查询所有已登记设备")
    args = cli.parse_args()
    request = {"op": args.op}
    if args.op == "read":
        if args.target is None:
            request["all"] = True
        else:
            request["target"] = args.target
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(90)
            connection.connect("/run/qintopia-tuya-sensors.sock")
            connection.sendall(json.dumps(request, ensure_ascii=False).encode() + b"\n")
            with connection.makefile("rb") as stream:
                data = json.loads(stream.readline(1024 * 1024))
        if not isinstance(data, dict) or "ok" not in data:
            raise ValueError
    except (OSError, ValueError):
        data = {"ok": False, "error": "本机传感器查询服务暂不可用。"}
    print(json.dumps(data, ensure_ascii=False, indent=2))
    return 0 if data["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
