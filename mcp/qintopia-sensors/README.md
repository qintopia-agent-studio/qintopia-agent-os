# 秦托邦传感器 MCP

Hermes 的 `qintopia-sensors` 暴露 `read_environment(area?)` 和
`list_sensor_areas()`。两个工具调用本机 CLI，CLI 通过 Unix
socket 请求共享查询服务。查询服务从涂鸦云读取已登记设备的属性，并按物模型返回数值、单位和北京时间。分别传入「一楼壁炉区」和「烟雾报警器」可查询区域和单台设备。

## Production Boundary

本包接管现行四个程序的源码。设备注册表由 `TUYA_DEVICES_FILE` 指定，服务从 systemd 的
`CREDENTIALS_DIRECTORY` 读取
`tuya.json`。线上 secrets、设备 ID 和云端状态保留在主机。MCP 调用方只能读取清单和状态。单个设备或属性查询失败时，仍保留成功的结果。

## Validation

在仓库根目录运行以下检查。

```sh
python3 -B -m unittest discover -s mcp/qintopia-sensors/tests -v
```

测试使用模拟设备注册表和云端结果。Hermes 回读、实际设备查询与生产切换属于独立的发布验收。

订阅适配器沿用现有接收器的 `pulsar-client==3.5.0` 和
`pycryptodome==3.23.0`。普通查询和离线规则测试只加载各自需要的依赖。

涂鸦的 `auth1` 使用 Pulsar
Basic 插入的冒号拼出认证 JSON。测试以模拟凭据核对报文、订阅名及连接释放。[涂鸦官方 Python 认证实现](https://github.com/tuya/tuya-pulsar-sdk-python/blob/main/mq_authentication.py)
