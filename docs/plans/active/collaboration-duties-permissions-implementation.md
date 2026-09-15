# 职责连接与分级权限：本地实现约定

用户已授权实现：少量权限类别明确授予，具体事项在职责与范围内自治，共同边界保持有效。本次补通用配置与判断，不连接真实训练、知识写入、PMS 或消息发送；不替实际责任人确定合作习惯。

## 用户操作与数据

- 秦托邦为顶层名称，“智能体”为统一对象名，测试状态单独显示。
- 职责单独维护名称、说明、工作领域与适用权限类别；岗位关联职责，可以新增、编辑、停用。
- 一条连接对应一个人、岗位、职责、智能体与范围。多人、多智能体、多职责通过多条完整连接表达。
- 连接中的每项权限为 autonomous（可自主决定）、confirmation（需指定人员确认）、denied（未授予）。confirmation 必须选择其他自然人，该人须在同智能体、职责和范围内具有此项自主决定权。
- 修改已有岗位／职责方案不得默默扩大存量授权；移除正被使用的职责／动作须先调整相关连接，返回影响信息。停用仍有当前任职或连接的对象也必须先处理关联；已用对象保留历史，未引用对象可删除。
- 可以结束单条连接，不影响同一人其他连接。换人／范围／智能体采用原子替换连接并保留历史，不自动结束其他任职。
- 同一来源的自主授权撤回或失效后，依赖它的授权和确认资格随之失效；无资格确认人不得降级为自主或改找任意人。
- 新职责支持无权限的配置草稿；提供信息不要求岗位授权。业务职责说明不直接决定执行权限。

## 本轮接口约定

沿用 `/api/state`、`/api/preview`、`/api/save` 的版本、幂等与事务审计。

state 增加以下字段：

- duties：id、label、description、domain、available_actions、status、version。
- roles：description、status、version、duty_ids。
- scopes：status。
- relations：duty、version、replaces、immutable。
- grants：mode、reviewer。

旧无 duty 的关系标记待关联职责，不自动扩大授权。

Assign 增加 duty（可空仅兼容旧本地测试）、permissions 数组。

每项为 action、mode、reviewer（confirmation 必填，其余为空）。旧 actions 仅保留既有测试兼容。

UI 只提交显式 duty 与 permissions；domain 从职责选择推导，仍由服务端核对。

新增命令：

- save_duty：id（新建为空）、label、description、domain、available_actions。
- save_role：id（新建为空）、label、description、duty_ids。
- retire_catalog：object（role/duty/scope）、id；有当前引用拒绝，历史引用停用，完全未用删除。
- update_scope：id、label；归属与群调整继续沿用受控现有入口，不隐式移动已有群。
- end_collaboration：collaboration。

`POST /api/decision` 接收 collaboration、action。

返回 status（autonomous/confirmation_required/denied）、reason、reviewer（适用时）、configuration_version。

这是当前配置解释与执行门禁的本地验证入口，不是可重放的批准或发送凭据。实际业务确认将由各流程引用内容、对象、版本与 WorkItem，本轮不把选择确认人当成已经批准一项业务动作。

## 工程边界与验证

追加 `.003` 迁移，保留已应用 `.001/.002`
校验和。复用 Person、任职、授权和审计，隔离库内验证。现有共用 policy 的宽维度查询遇到多职责歧义或需确认权限时不能提供自主许可；新判断绑定完整连接。界面为完整连接行与对象详情，岗位、职责、范围分别维护，权限三态逐项说明，不增加生活事项白名单。

测试涵盖：自主／确认／未授予，确认人撤权与到期、自我确认、跨职责与跨范围拒绝，角色方案不自动赋权、缩减引用冲突、停用／删除历史、单连接结束、连接替换与并发幂等。最后进行浏览器操作与响应式检查，明确本地可用范围，保持外部能力关闭。
