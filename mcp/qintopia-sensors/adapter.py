"""Expose the host's registered sensor readings to Hermes over stdio."""
import json
import subprocess

from mcp.server.mcpserver import MCPServer
from mcp.types import ToolAnnotations


server = MCPServer("qintopia-sensors", log_level="WARNING")
read_only = ToolAnnotations.model_validate({"readOnlyHint": True, "destructiveHint": False,
                                            "idempotentHint": True, "openWorldHint": False})


def query(*arguments):
    try:
        process = subprocess.run(["/usr/local/bin/qintopia-sensors", *arguments],
                                 capture_output=True, text=True, timeout=95)
        result = json.loads(process.stdout)
        if isinstance(result, dict) and isinstance(result.get("ok"), bool):
            return result
    except (OSError, subprocess.TimeoutExpired, ValueError):
        pass
    return {"ok": False, "error": "腾讯云本机的传感器读取服务暂不可用。"}


@server.tool(annotations=read_only)
def read_environment(area: str | None = None) -> dict:
    """查询秦托邦区域温湿度、烟雾和一氧化碳状态及各自上报时间。area 可填区域或设备名称，省略则查询全部。"""
    if area is None:
        return query("read", "--all")
    if not area.strip() or len(area) > 128:
        return {"ok": False, "error": "请填写已登记的区域或设备名称，或省略 area 查询全部。"}
    return query("read", "--", area.strip())


@server.tool(annotations=read_only)
def list_sensor_areas() -> dict:
    """列出秦托邦已登记的传感器名称与区域，供查询新地点时使用。"""
    return query("list")


if __name__ == "__main__":
    server.run(transport="stdio")
