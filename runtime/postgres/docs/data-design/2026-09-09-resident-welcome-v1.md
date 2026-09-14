# 统一人员与欢迎 V1 本地实施

Owner: PatrickLiveCool。风险：high。状态：本地开发，生产未启用。

唯一协议见[共同契约](../../../../docs/plans/active/unified-person-welcome-v1-contract.md)。本设计不复制 PMS 协议，也不替 PMS 实现 Outbox 或住宿事实。

## 切片与数据所有权

2026-09-09 用户纠偏：长期任职、训练授权及协作找人须属于 Agent
OS 通用人员基础设施，欢迎只是消费者。唯一规格见共同契约第 5.3 节。下文及现有
`welcome_grants`
描述的是此前欢迎领域草稿实现，不代表已满足通用授权要求；不得将其直接上线或把原欢迎权限提升为全域权限。后续重构须整合既有训练授权入口，并分别验证跨智能体/楼栋边界与离任交接。

- A1：扩展现有 Person 的有类型来源链接；申请、逐入住人案例、授权与撤销。复用
  `qintopia_identity.persons`、`channel_identities`、`person_memberships`，不以名称确认身份。稳定 case 不随 Person 合并而变化。
- A2：原始字节验签、Inbox 原子去重、检查点与聚合版本、claim
  fencing。查询页全部持久接收后才推进游标；来源重建期间关闭自动发布。
- A3：认证操作者的范围授权、版本化人工确认/审核与成员名单完整性检查。不将邀请人的 senderId 作为新成员，也不将中转 ID 当官方 ID。
- A4：复用 Artifact、逐目标/phase/part 动作、持久尝试和未知回执恢复。卡片或路由版本变化只能使未发动作重新审核，不能使已发动作重发。

扩展表以 `welcome_`
前缀置于现有控制面 schema，外键指向已有 Person、WorkItem、Artifact。旧 channel
identity 未知 namespace 保留，不自动回填企业。新工作类型走专用领域服务，通用 Operations
MCP 的 preview 保持 preview。

## 事务与隔离

Inbox 接受和恢复 WorkItem 同事务；相同 source/event 不同原始 hash 返回冲突。同一来源/物业检查点锁保护 pull 页提交。Inbox
claim 与 WorkItem
claim 同步，递增 fence；过期处理者不能提交结果。业务 action 使用稳定 case/phase/target/part。外部调用前持久化 sending，崩溃后为 unknown，禁止自动重发。

本轮运行入口仅接受显式隔离的本地 `qintopia_test`
数据库，拒绝 live 数据库；影子环境复用相同 schema 的独立数据库，不给既有 live 人员/授权/任务添加影子行。

所有制卡、上传、PMS 写、消息发送在影子执行器中硬拒绝；测试执行器只允许合成 fixture。

不会建立生产服务、定时器、profile 或外部凭证配置。

## 隐私和生命周期

重建先持久记录 feed head、扫描代次和关闭状态，再按 afterId 扫描。

每页的校验投影与检查点同事务保存；重启从待处理投影恢复，绝不从缺失行推断删除。

扫描投影和事件共用状态处理器，前者不生成伪 PMS 事件、不授予欢迎准入。扫描完成后从保存的 head 补偿；重建完成也不自动打开发布。

周期回读负责实体失效、营业日变化和提交后丢失唤醒，逐来源/物业领取 Inbox。同向量冲突记录独立 conflicted 状态，不因重复读到旧版本自动解封。

事件仅保存校验后的最小引用与原始 hash，不保存未知字段或投递 headers。

人类操作审计只保存内部引用、动作、修订和证据引用，不记录原话或私密资料。

申请使用受控源引用、修订、同意版本和许可投影；生产字段白名单与保留策略未配置时不授权。

测试全部合成。

生产清理须按批准保留期执行，不删除未解决 unknown。

## 验证及回滚

按共同契约 T01—T16 建立包级测试及隔离 Postgres 事务测试。运行 `pnpm check:pr:heavy`
或等价专项检查，完成前执行
`git diff --check`。T07/T08 源事务保证需 PMS 自身测试；本仓只验证模拟 feed 消费与收敛。

迁移为追加结构，不改现有 worker 查询语义；不自动授权 capability。

回滚停新接收/处理/外部执行，保留 Inbox、case、action 和 unknown 证据；不得删除数据后重放积压。

部署、真实接收、真实附件和群发送均需后续单独授权。
