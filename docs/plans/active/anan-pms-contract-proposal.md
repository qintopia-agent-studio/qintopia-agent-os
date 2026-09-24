# 岸岸能力登记与共同执行权限：专项批准方案

日期：2026-09-24。基线 `b42db0d`，继承规格提交
`adff875`。状态：用户已于 2026-09-24 明确批准下列 A 最小范围。批准来源：总指挥任务
`01a0c6ee-1de7-7302-8be1-3f7966f51bea` 中负责人的“同意”，由总指挥转交执行。批准覆盖 1
manifest
schema、2 检查器、1 回归文件、1 既有入口追加、2 处契约文档及正常 Agent 登记；不包含 resolver、restart
rules、安装 payload 或生产启用。

## A. 能力身份登记与受管部署分离

证据：`runtime/sidecar/src/person_collaboration/model.rs::agents()` 从现有
`registry/agents.yaml` 读取身份。`authorize_current` 拒绝未登记 Agent。
`tools/agents/check-agents.mjs` 第 113—125 行要求每个非 default
Agent 都具有 restart/service；`docs/engineering/change-routing-index.md`
第 110—113 行要求新 Agent 同步部署 schema、smoke、restart
rule。当前岸岸尚未接管用户 Profile，不能虚构受管服务，也不能借 fixture 绕过。

建议在现有 package manifest 的 `runtime` 中增加通用 `management` 状态：`managed` /
`unmanaged`。缺省继续按现有 managed 契约解释；unmanaged 只承认能力身份，不声明生产可用。unmanaged 分支必须禁止 restart/service 字段，且其 Agent 不得出现于 Hermes
Profile registry、restart
rules、生产请求 target 枚举或安装清单。unmanaged 仅表示未由 Agent
OS 接管发布，不表示服务器没有该 Agent 或 Profile。状态升级为 managed 仍要求完整部署契约及单独审批，不因登记自动升级。

具体拟改文件与数量：

| 文件                                                   | 拟改内容                                                                                                                                                                         | 数量                          |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------- |
| `registry/schemas/package-manifest.schema.json`        | runtime 使用两种严格形状；兼容缺省 managed，拒绝 unmanaged 携带部署目标                                                                                                          | 1 个 schema 规则组            |
| `tools/agents/check-agents.mjs`                        | 按通用 management 验证现有 runtime 标识；unmanaged 验证未进入部署白名单；保留所有其他包要求                                                                                      | 1 个检查器、2 个分支          |
| `tools/deploy/check-deploy-runner.mjs`                 | 第 2933—2967 行的 registry 遍历使用同一通用状态契约；managed 原有 schema/rule/smoke/service 检查全部保留，unmanaged 明确检查无目标/服务且不在已核对生产清单，不能无条件 continue | 第 2 个检查器、2 个分支       |
| `tools/agents/test-agent-runtime-management.mjs`（新） | 模拟独立仓库 fixture 验证 managed 原行为、unmanaged 隔离、错误形状、意外白名单污染                                                                                               | 1 个包内检查测试文件          |
| `package.json` 的既有 `agents:check`                   | 在原检查后调用上述回归，不加 CI job                                                                                                                                              | 1 个现有检查入口追加 1 条命令 |
| `docs/engineering/package-contract.md`                 | 状态、升级与回滚契约                                                                                                                                                             | 1 处文档                      |
| `docs/engineering/change-routing-index.md`             | 只有 managed 登记触发完整部署契约；unmanaged 不可部署                                                                                                                            | 1 处文档                      |
| `agents/anan/` 与 `registry/agents.yaml`               | 按原包要求新建正式身份/能力声明，runtime.management=unmanaged；不复制欢迎 fixture                                                                                                | 1 个 Agent 正常登记           |

部署配置、workflow、实际安装器、restart rules、Profile registry 和生产白名单：拟修改数量
**0**。检查器读取现有清单并拒绝污染，不能把 unmanaged 跳过所有验证。实施前继续核对真实安装清单表示方式；若需扩大上述文件/规则范围，先补差异，不自行实施。

按 CI 护栏逐项评审：

1. 必要性：已授权本地能力无法进入共同授权；强行登记会虚构已接管生产 Profile。
2. 数量：0 job、0 workflow、0 依赖、1
   schema 规则组、2 检查器、1 现有入口追加、1 回归文件；业务登记和文档如上。
3. 依据：上述源码与合并版仅保留岸岸职责文档的事实；不是为了消除业务测试失败而降断言。
4. 复杂度：仅两状态，无新 registry、无发现服务、无单 Agent 例外。缺省兼容避免批量迁移既有 Agent。
5. 现规则不足：缺乏“身份存在但尚未受管”的状态。只改业务包不能使共同身份合法，借其他 Agent 则错误归责。
6. 最小性：不改部署执行路径；只在登记校验层区分状态并证明未接管者不能部署。
7. 通用性：按 manifest 状态适用于所有 Agent，不检查 anan 名称来豁免，不降低既有受管 Agent 要求。
8. 部署边界：业务测试留在开发检查，不进入生产部署流程。
9. 复用价值：后续用户已有 Profile 的渐进接管均可沿用；显式状态防止“登记即上线”的错误推断。

替代方案：等待岸岸完整受管发布后才做共同授权，会把本地独立交付绑定生产；复制第二 Agent 注册表或硬编码 anan 会漂移；登记假的 restart/service 会误导运维。均不建议。

兼容与验证：现有七个 Agent 原检查结果、部署 target 集合与安装行为必须保持；新增测试验证未知 management 拒绝、managed 缺标识拒绝、unmanaged 带标识拒绝、混入已核对生产清单拒绝。运行
`pnpm agents:check`、registry、profile-bundles、deploy contracts/runner、定向测试与适用
`pnpm check:pr:auto`。回滚新身份登记前先停本地入口并保留事项；不删除用户实际 Profile。

### 发布路径实查与尚未扩大的范围

- `tools/agents/check-profile-bundles.mjs:278—307`
  会遍历新登记包的 Profile 模板，检查 runtime mounts 和
  `.env`、sessions、logs、cache、state.db 排除；不自动安装 Profile。继续执行原检查，无需修改。
- `tools/deploy/build-deploy-bundle.mjs` 使用明确文件/目录 allowlist，目前没有
  `agents/anan/` 或
  `skills/pms-operations/`，登记本身不会把它们打包或安装。A 不扩展 payload。
- `tools/deploy/resolve-restart-targets.mjs:156—186` 按路径匹配，不读取 Agent
  management。新增包内非 Markdown 源码会落入 `production_adjacent_paths`
  的 unmatched，不能因为 unmanaged 就声称无需评审或绕过失败。
- A 的安全反向检查范围明确为：`runtime/hermes/profile-registry.yaml`、`deploy/restart-target-rules.yaml`
  的 targets/rules、`deploy/runner/deploy-request.schema.json` target 枚举及
  `deploy/runner/smoke-release.sh`
  对应服务目标；不承诺覆盖尚未枚举的“任意安装清单”。构建 payload
  allowlist 保持现状并用读回断言验证岸岸包未被加入。

A 可以只交付身份登记，新增能力源码保持未发布且 release
resolver 继续明确阻断；发布归属不能借 no_restart_paths 的 anan 特例解决。若用户希望本轮同时解除该阻断，需要另审通用的“非发布能力包”路径处置方案，

明确是否打包、谁引用及后续升级门禁；本次 A 文件表没有隐含授权修改 resolver 或 restart
rules。

回归通过两个真实检查入口执行分支：在一次隔离模拟仓库 fixture 中并列多个通用 Agent 状态及非法组合，运行两个入口并核对各自错误集合，再运行一个全部合法状态集合。

每个入口只需正常/错误两轮，不为每个例子重复完整部署检查，也不把测试降成只调用自写辅助函数。实际仓库再运行一次既有完整检查证明七个受管 Agent 行为未变。

## B. PMS 当次权限映射及真实缺口

现有 action 是 confirm_knowledge、train、change_rules、review、publish、designate、identity、

manage、technical_support。这些分别约束知识、训练、规则、审核、发布、指定、身份、组织管理与技术支持；没有可直接表示订单/收款业务执行的 action。
**本批不把任何 PMS 资金或房态动作映射为 manage，也不把 review 当作执行权。**

| 实际操作                          | 可复用现有 action                | 人员任职与范围                                     | PMS 执行主体上限                                                                   |
| --------------------------------- | -------------------------------- | -------------------------------------------------- | ---------------------------------------------------------------------------------- |
| PMS 房源/订单/成员/流水查询、报价 | 无合适业务读取 action；记为缺口  | 当前有效客房职责、hospitality、物业及目的/受众范围 | propertyAccess READ/WRITE；报价 CREATE_QUOTE 协议                                  |
| 订房                              | 无合适业务执行 action；记为缺口  | 有权客服在该物业办理订单                           | CREATE_ORDER 同时在 allowedActions 与 propertyCommandGrants                        |
| 登记收款                          | 同上；不是 review/publish/manage | 该物业收款登记职责，核实真实流水                   | RECORD_COLLECTION 精确 grant                                                       |
| 实际入住/退房                     | 同上                             | 该物业住宿办理职责，独立到店依据                   | CHECK_IN / CHECK_OUT 各自 grant                                                    |
| 改期/续住/缩住/换房/取消          | 同上，各动作分别限制             | 该物业明确授予的住宿调整范围                       | RESCHEDULE_STAY / EXTEND_STAY / SHORTEN_STAY / MOVE_UNIT / CANCEL_ORDER 各自 grant |
| 经营规则维护                      | change_rules / confirm_knowledge | 主管或四老师获授权的规则范围                       | 不以此授予 PMS 写入                                                                |
| 欢迎内容审核/发布                 | 既有 review / publish            | 按现有欢迎契约                                     | 不授予 PMS 写入                                                                    |

建议最小共享语义扩展是通用 `read_business` 与 `execute_business`
两个 action，仍归 hospitality 大类和既有岗位职责；执行授权需额外绑定已登记 capability
key 集合或等效的精确操作范围，不能用一个 execute_business 放行全部资金/住宿动作。现有 collaboration_grants 未表达这项执行范围，

总指挥已于本轮确认该共同业务语义属于已授权本地共享增量；将以具体数据设计落实精确操作范围，缺省拒绝和子集委派。A 按上述用户专项批准执行。

人员权限、岸岸工具集合、PMS
Token/物业 grant 三者每次相交。普通员工获得精确业务授权后，无需额外主管逐笔审批；人类确认绑定具体方案，PMS 技术 Confirm 不制造第二次业务批准。当前缺口解决前，

服务写入入口应返回明确未授权，不允许客户端自身宣称已完成端到端授权。
