# 秦托邦环境监测与报警

四老师和二花使用同一份
[Hermes Skill](SKILL.md)。按地点查询温度、相对湿度、烟雾和一氧化碳时，调用同一个只读服务。每个属性有独立的设备上报时间。

## 包归属

- 本包维护查询说明和[群通知文案](references/alarm-messages.json)。两位智能体的 profile 声明使用权限。
- [传感器 MCP](../../mcp/qintopia-sensors/README.md) 维护查询接口、设备映射和数据换算。
- [壁炉区报警工作流](../../workflows/fireplace-alarm/README.md)
  根据设备主动上报处理撤离、重复提醒、恢复和发送回执。

## 发布状态

现有线上 profile 已能查询传感器。本次登记的源码及报警工作流供审阅。上线需要单独发布，实际设备清单、群地址和凭据由主机管理。
