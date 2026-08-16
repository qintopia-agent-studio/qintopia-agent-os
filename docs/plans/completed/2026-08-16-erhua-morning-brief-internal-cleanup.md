# Erhua Morning Brief 内部整理

Date: 2026-08-16 Branch: `refactor/erhua-morning-brief-internal-cleanup` Affected
domain: `workflows/erhua-morning-brief`

Status: **Completed** (2026-08-16, PR #609) — all six tasks implemented; 28 workflow
tests pass, workflow + deploy contract checks pass.

## Goal

在不改变外部行为、生产边界或输出格式的前提下，整理 `workflows/erhua-morning-brief/`
内部的重复代码、文档不一致和测试 warning。

## Scope

仅修改以下文件：

- `workflows/erhua-morning-brief/workflow.yaml`
- `workflows/erhua-morning-brief/morning_brief.py`
- `workflows/erhua-morning-brief/tests/test_morning_brief.py`
- `workflows/erhua-morning-brief/README.md`（一致性检查）

## Changes

1. **修复依赖声明**
   - `workflow.yaml` 当前依赖 `workflows/xiaoman-weekly-preview`，但 `morning_brief.py`
     实际调用的是 `skills/qintopia-tools/variants/xiaoman/` 中的
     `handle_qintopia_xiaoman_activity_announcement_prepare`。
   - 改为依赖 `skills/qintopia-tools` 和 `skills/xiaoman-activity`。

2. **消除重复的环境变量 fallback 解析**
   - 三个 `--news-llm-*` 参数都有重复的
     `os.environ.get(specific, os.environ.get(shared, ""))` 嵌套。
   - 新增 `_env_with_fallback(specific_key, shared_key, default="")` helper 替换。

3. **合并重复的 sidecar action 执行逻辑**
   - `_artifact_create_action` 和 `_send_request_action` 结构几乎相同。
   - 新增 `_run_sidecar_action(command, *, execute, apply, error_message)` helper 复用。

4. **修复测试 ResourceWarning**
   - `NoNewsFeedRedirect.redirect_request` 构造 `HTTPError` 时传入 `fp`，测试结束后触发
     `ResourceWarning`。
   - 构造 `HTTPError` 时不传递 `fp`。

5. **简化测试中的环境变量管理**
   - `test_news_llm_args_fall_back_to_shared_llm_env` 手动保存/恢复环境变量。
   - 改为 `unittest.mock.patch.dict(os.environ, {...}, clear=False)`。

6. **README 一致性检查**
   - 确认 `--news-llm-*` fallback 到 `QINTOPIA_LLM_*` 的说明与代码一致。

## Validation

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/erhua-morning-brief/tests -v`
- `pnpm workflows:check`
- `pnpm check:pr:quick`
- `pnpm lint:md`（若 README 有改动）

## Production Boundary

- 不触及外部发送、数据库写入、runtime profile、secrets。
- 只调整内部 helper、依赖声明和测试写法。
