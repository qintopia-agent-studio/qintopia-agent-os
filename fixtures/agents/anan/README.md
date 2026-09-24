# 岸岸本地测试替身

这些脚本和模板只供人员底座及欢迎链的合成测试使用，不是已创建的 Hermes 岸岸 Profile，不登记到生产 Agent 清单。真实 Runtime 由后续联调核对，不能用此模板覆盖。

源文件来自提交 `3030d67` 的
`agents/anan/`。迁移到 fixtures 是为了让测试资产遵循既有非部署路径规则，不为草稿增加部署门禁例外。保留独立工具注册、严格任务输入、源码摘要和外部动作禁用检查。

验证：`python3 -m unittest discover -s fixtures/agents/anan/tests -v`。
