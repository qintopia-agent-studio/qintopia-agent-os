# 岸岸来源与继承

正式能力规格继承自 PR #720 合并基线 `b42db0d` 上的文档补丁，独立提交
`adff875`。业务计划位于 `docs/plans/active/anan-pms-event-integration.md`。

Green PMS 参考基线
`0254fbabdda0b76b56370248f2ad24e44e4e950a`，只读复用其命令、权限和恢复契约。Hermes 官方 WeCom 本地源码观察基线
`d337b736aa1e8ebecfab043842d13e4a2d2f48a3`；本地源码观察不证明服务器相同。

用户已建立的 Profile、SOUL、Livecool、渠道、会话和凭据属于 runtime-only，未读取或覆盖。
`fixtures/agents/anan/`
保持模拟欢迎来源，未搬回。正式 Plugin 位于能力包，不登记测试运行器为生产组件。
