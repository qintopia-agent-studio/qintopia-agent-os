# 通用图片发图模块设计方案

> 背景：项目有 3 个「生成图片发群」场景（AI画图 / 小满日报 / 二花早报），发图链路目前是重复代码。本方案把它抽成通用模块，早报发图直接复用，同时消除画图/日报的重复。配合日报 Python→Rust 迁移。

## 核心结论

发图链路有 **5 处重复**，其中最大的两处：

1. **Feishu 图片存储函数逐字节拷贝**：`huabaosi_feishu_artifact_mirror.rs` 的
   `store_primary_generated_image`(2123) 和
   `store_daily_case_report_image`(2194) 函数体完全一致，仅 3 处差异（尺寸校验策略、个别入参字段、Base 字段模板常量）。
2. **image-send claim 的 SQL 被复制 4 处**：`qiwe_image_send_state.rs`
   的 claim/preview/lock_current_claim/lock_callback_policy 四个函数里，workflow 认领分支逐字重复，加早报要改 4 个地方（极易漏）。

## 通用模块设计（3 个模块）

- **`media_identity.rs`（新）**：JPEG
  identity 计算/校验 + 哈希原语 + 确定性 UUID。纯函数无 IO，最易抽。
- **`media_upload.rs`（新）**：存储后端抽象（HTTP 公开存储 + Feishu
  Base），合并两个 store 函数为一个 `store_feishu_image(profile)`。三方差异用
  `ImageWorkflowProfile` enum 表达（HuabaosiGenerated / XiaomanDailyCaseReport /
  ErhuaMorningBrief）。
- **`qiwe_image_send_state.rs`（改）**：claim SQL 的 workflow 分支抽成一个
  `CLAIMABLE_IMAGE_SOURCES` 数组统一渲染，4 处 SQL 自动一致，加早报只改一处。

## 关键兼容原则（不破坏生产）

- **字节级等价重构**：合并后的字段 JSON 与现状完全一致，既有单测不改断言全绿。
- **idempotency
  / 确定性 UUID 的 seed 字符串原样搬运**（是线上数据关联键，改一个字符就重发/撞键）。
- **HTTP 路径不强行统一**：日报 HTTP 不做字节读回（现状），画图的读回保留为画图专属——不为统一而改生产行为。
- **claim
  SQL 只增不改**：早报分支是 OR 新增，现有画图/日报两段子句文本不变，加 snapshot 测试锁等价。
- **feature gate 原样保留**：Feishu 存储的 fail-closed 门不变。

## 实施 PR 拆分（每个独立可合并、不破坏生产）

| PR                                   | 内容                                                   | 风险                           |
| ------------------------------------ | ------------------------------------------------------ | ------------------------------ |
| PR-1 `media_identity.rs`             | 抽哈希/UUID/JPEG identity 原语，两方改调用             | 低（纯函数搬运）               |
| PR-2 `media_upload.rs` Feishu 公共化 | 合并两个 store 为一个，旧函数变薄包装（调用方零改动）  | 中                             |
| PR-3 调用方直连 + HTTP 公共化        | 画图/日报直连新模块，删旧拷贝                          | 中（动两个生产调用点）         |
| PR-4 claim SQL 参数化                | 数组统一渲染 4 处 SQL，snapshot 锁等价                 | 中高（发送核心，但语义零变更） |
| PR-5 早报发图接入                    | worker 渲染 + 新 Rust 命令 + claim 加早报 + 发送改发图 | 新增功能，独立上线             |

PR-1~4 完成画图+日报统一；PR-5 依赖 PR-1/2/4，可独立推迟。

## 早报接入（PR-5）

照小满日报链路平行接：

1. worker bash 加 `--render-image` 渲染卡片
2. 新增 Rust 命令
   `operations-erhua-morning-brief-media-upload`（调通用 media_upload 模块）
3. `CLAIMABLE_IMAGE_SOURCES`
   加早报分支（review_policy 建议沿用 human_final_confirmation，与现有 confirm 步骤一致，风险最低）
4. worker 发送段从 qiwe-text-send 换成 qiwe-image-send 链

## 待拍板的点

1. 早报 generated_image artifact 的 `created_by_agent`
   用哪个（需与现有 artifact 创建命令对齐）
2. 早报的 `source_work_item_type` / `capability_key`
   命名（复用现有还是新注册，需查 operations 现有注册）
3. 是否本次就做 PR-1~4 的彻底重构，还是只做 PR-5 需要的最小抽离

（完整调研证据见会话内 Plan 分析，含全部文件+行号）
