# Erhua Morning Brief 内部整理实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox
> (`- [ ]`) syntax for tracking.

**Goal:** 在不改变外部行为、生产边界或输出格式的前提下，整理
`workflows/erhua-morning-brief/` 内部的重复代码、文档不一致和测试 warning。

**Architecture:** 通过提取两个小型内部 helper（环境变量 fallback、sidecar
action 执行）消除重复；修正 `workflow.yaml` 依赖声明以匹配实际调用关系；修复
`NoNewsFeedRedirect` 中的 `ResourceWarning`；简化测试中的环境变量管理。

**Tech Stack:** Python 3, `unittest`, `unittest.mock`, `argparse`, `urllib.request`

---

## File Structure

| File                                                        | Responsibility                                            |
| ----------------------------------------------------------- | --------------------------------------------------------- |
| `workflows/erhua-morning-brief/workflow.yaml`               | 声明 workflow 元数据、依赖和验证命令                      |
| `workflows/erhua-morning-brief/morning_brief.py`            | 主要业务逻辑；新增两个内部 helper 并修复 redirect handler |
| `workflows/erhua-morning-brief/tests/test_morning_brief.py` | 单元测试；新增 helper 测试并简化环境变量测试              |
| `workflows/erhua-morning-brief/README.md`                   | 运行说明；检查 fallback 描述一致性                        |

---

## Task 1: Fix `workflow.yaml` dependency declaration

**Files:**

- Modify: `workflows/erhua-morning-brief/workflow.yaml`
- Test: `pnpm workflows:check`

- [ ] **Step 1: Read current dependencies**

  Read `workflows/erhua-morning-brief/workflow.yaml` lines 27-33.

- [ ] **Step 2: Replace inaccurate dependency**

  Replace:

  ```yaml
  dependencies:
    - skills/qintopia-tools
    - workflows/xiaoman-weekly-preview
    - runtime/sidecar
  ```

  With:

  ```yaml
  dependencies:
    - skills/qintopia-tools
    - skills/xiaoman-activity
    - runtime/sidecar
  ```

  Keep the remaining dependencies (`agents/erhua`, `agents/xiaoman`, `qunmind`)
  unchanged.

- [ ] **Step 3: Validate**

  Run:

  ```bash
  pnpm workflows:check
  ```

  Expected: `Workflow check passed.`

- [ ] **Step 4: Commit**

  ```bash
  git add workflows/erhua-morning-brief/workflow.yaml
  git commit -m "docs(workflows): correct erhua-morning-brief dependencies"
  ```

---

## Task 2: Extract `_env_with_fallback` helper

**Files:**

- Modify: `workflows/erhua-morning-brief/morning_brief.py`
- Modify: `workflows/erhua-morning-brief/tests/test_morning_brief.py`

- [ ] **Step 1: Add failing test for the new helper**

  Add to `workflows/erhua-morning-brief/tests/test_morning_brief.py`:

  ```python
  def test_env_with_fallback_prefers_specific_key(self):
      module = load_module()
      with unittest.mock.patch.dict(
          os.environ,
          {
              "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL": "specific",
              "QINTOPIA_LLM_BASE_URL": "shared",
          },
          clear=False,
      ):
          self.assertEqual(
              module._env_with_fallback(
                  "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL",
                  "QINTOPIA_LLM_BASE_URL",
              ),
              "specific",
          )

  def test_env_with_fallback_uses_shared_key_when_specific_missing(self):
      module = load_module()
      with unittest.mock.patch.dict(
          os.environ,
          {"QINTOPIA_LLM_BASE_URL": "shared"},
          clear=False,
      ):
          self.assertEqual(
              module._env_with_fallback(
                  "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL",
                  "QINTOPIA_LLM_BASE_URL",
              ),
              "shared",
          )

  def test_env_with_fallback_returns_default_when_both_missing(self):
      module = load_module()
      with unittest.mock.patch.dict(
          os.environ,
          {},
          clear=False,
      ):
          self.assertEqual(
              module._env_with_fallback(
                  "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL",
                  "QINTOPIA_LLM_BASE_URL",
              ),
              "",
          )
  ```

- [ ] **Step 2: Run tests to verify they fail**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest workflows/erhua-morning-brief/tests/test_morning_brief.py -v
  ```

  Expected: 3 failures mentioning `_env_with_fallback` is not defined.

- [ ] **Step 3: Implement `_env_with_fallback`**

  Add to `workflows/erhua-morning-brief/morning_brief.py` near the top-level utility
  functions:

  ```python
  def _env_with_fallback(specific_key: str, shared_key: str, default: str = "") -> str:
      return os.environ.get(specific_key, os.environ.get(shared_key, default))
  ```

- [ ] **Step 4: Refactor `--news-llm-*` argument defaults**

  In `_parse_args`, replace the three argument default blocks with:

  ```python
  parser.add_argument(
      "--news-llm-base-url",
      default=_env_with_fallback(
          "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL",
          "QINTOPIA_LLM_BASE_URL",
      ),
      help="Optional OpenAI-compatible endpoint used to translate English RSS items for the community brief.",
  )
  parser.add_argument(
      "--news-llm-api-key",
      default=_env_with_fallback(
          "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_API_KEY",
          "QINTOPIA_LLM_API_KEY",
      ),
      help="Optional API key for the news translation endpoint.",
  )
  parser.add_argument(
      "--news-llm-model",
      default=_env_with_fallback(
          "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_MODEL",
          "QINTOPIA_LLM_MODEL",
      ),
      help="Optional model name for the news translation endpoint.",
  )
  ```

- [ ] **Step 5: Run tests to verify they pass**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/erhua-morning-brief/tests -v
  ```

  Expected: All tests pass.

- [ ] **Step 6: Commit**

  ```bash
  git add workflows/erhua-morning-brief/morning_brief.py
  git add workflows/erhua-morning-brief/tests/test_morning_brief.py
  git commit -m "refactor(workflows): extract env fallback helper for news llm config"
  ```

---

## Task 3: Extract `_run_sidecar_action` helper

**Files:**

- Modify: `workflows/erhua-morning-brief/morning_brief.py`
- Modify: `workflows/erhua-morning-brief/tests/test_morning_brief.py`

- [ ] **Step 1: Add failing test for the new helper**

  Add to `workflows/erhua-morning-brief/tests/test_morning_brief.py`:

  ```python
  def test_run_sidecar_action_returns_preview_when_not_executed(self):
      module = load_module()
      args = SimpleNamespace(execute_artifact_create=False, apply_artifact_create=False)
      command = ["echo", "hello"]
      action = module._run_sidecar_action(
          command,
          args=args,
          execute_flag="execute_artifact_create",
          apply_flag="apply_artifact_create",
          error_message="sidecar action failed",
      )
      self.assertEqual(action["command"], command)
      self.assertEqual(action["shell_preview"], "echo hello")
      self.assertFalse(action["execute_requested"])
      self.assertFalse(action["apply_requested"])
      self.assertNotIn("returncode", action)

  def test_run_sidecar_action_executes_command_when_requested(self):
      module = load_module()
      args = SimpleNamespace(execute_artifact_create=True, apply_artifact_create=True)
      command = [sys.executable, "-c", "print('{\"ok\": true}')"]
      action = module._run_sidecar_action(
          command,
          args=args,
          execute_flag="execute_artifact_create",
          apply_flag="apply_artifact_create",
          error_message="sidecar action failed",
      )
      self.assertEqual(action["returncode"], 0)
      self.assertIn("ok", action["stdout"])

  def test_run_sidecar_action_raises_on_failure(self):
      module = load_module()
      args = SimpleNamespace(execute_artifact_create=True, apply_artifact_create=True)
      command = [sys.executable, "-c", "import sys; sys.exit(1)"]
      with self.assertRaisesRegex(RuntimeError, "sidecar action failed"):
          module._run_sidecar_action(
              command,
              args=args,
              execute_flag="execute_artifact_create",
              apply_flag="apply_artifact_create",
              error_message="sidecar action failed",
          )
  ```

- [ ] **Step 2: Run tests to verify they fail**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest workflows/erhua-morning-brief/tests/test_morning_brief.py -v
  ```

  Expected: 3 failures mentioning `_run_sidecar_action` is not defined.

- [ ] **Step 3: Implement `_run_sidecar_action`**

  Add to `workflows/erhua-morning-brief/morning_brief.py` above
  `_artifact_create_action`:

  ```python
  def _run_sidecar_action(
      command: list[str],
      *,
      args: argparse.Namespace,
      execute_flag: str,
      apply_flag: str,
      error_message: str,
  ) -> dict[str, Any]:
      execute_requested = bool(getattr(args, execute_flag, False))
      apply_requested = bool(getattr(args, apply_flag, False))
      action: dict[str, Any] = {
          "payload": None,  # populated by callers when needed
          "command": command,
          "shell_preview": " ".join(shlex.quote(part) for part in command),
          "execute_requested": execute_requested,
          "apply_requested": apply_requested,
          "external_send_executed": False,
      }
      if not execute_requested:
          return action

      completed = subprocess.run(command, check=False, capture_output=True, text=True)
      action["returncode"] = completed.returncode
      action["stdout"] = completed.stdout
      action["stderr"] = completed.stderr
      if completed.returncode != 0:
          raise RuntimeError(error_message)
      return action
  ```

- [ ] **Step 4: Refactor `_artifact_create_action` and `_send_request_action`**

  Replace `_artifact_create_action` with:

  ```python
  def _artifact_create_action(args: argparse.Namespace, payload: dict[str, Any]) -> dict[str, Any]:
      command = _artifact_create_command(args, payload)
      action = _run_sidecar_action(
          command,
          args=args,
          execute_flag="execute_artifact_create",
          apply_flag="apply_artifact_create",
          error_message="operations-text-announcement-artifact-create failed",
      )
      action["payload"] = payload
      return action
  ```

  Replace `_send_request_action` with:

  ```python
  def _send_request_action(args: argparse.Namespace, payload: dict[str, Any]) -> dict[str, Any]:
      command = _operations_create_command(args, payload)
      action = _run_sidecar_action(
          command,
          args=args,
          execute_flag="execute_send_request",
          apply_flag="apply_send_request",
          error_message="operations-work-item-create failed for Erhua morning brief send request",
      )
      action["payload"] = payload
      return action
  ```

- [ ] **Step 5: Run tests**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/erhua-morning-brief/tests -v
  ```

  Expected: All tests pass.

- [ ] **Step 6: Commit**

  ```bash
  git add workflows/erhua-morning-brief/morning_brief.py
  git add workflows/erhua-morning-brief/tests/test_morning_brief.py
  git commit -m "refactor(workflows): extract shared sidecar action helper"
  ```

---

## Task 4: Fix `NoNewsFeedRedirect` ResourceWarning

**Files:**

- Modify: `workflows/erhua-morning-brief/morning_brief.py`
- Modify: `workflows/erhua-morning-brief/tests/test_morning_brief.py`

- [ ] **Step 1: Reproduce the warning**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest workflows.erhua-morning-brief.tests.test_morning_brief.ErhuaMorningBriefTests.test_news_feed_redirect_handler_rejects_redirects -v
  ```

  Expected: Test passes with a
  `ResourceWarning: Implicitly cleaning up <HTTPError 302 ...>`.

- [ ] **Step 2: Fix the redirect handler**

  In `workflows/erhua-morning-brief/morning_brief.py`, replace `NoNewsFeedRedirect`
  with:

  ```python
  class NoNewsFeedRedirect(urllib.request.HTTPRedirectHandler):
      def redirect_request(self, req, fp, code, msg, headers, newurl):
          raise urllib.error.HTTPError(
              req.full_url,
              code,
              "news feed redirects are not allowed",
              headers,
              None,
          )
  ```

  Change `fp` argument to `None` so the raised error does not hold a reference to the
  response body.

- [ ] **Step 3: Add a regression test for no ResourceWarning**

  Add to `workflows/erhua-morning-brief/tests/test_morning_brief.py`:

  ```python
  def test_news_feed_redirect_handler_does_not_leak_file_pointer(self):
      module = load_module()
      handler = module.NoNewsFeedRedirect()
      request = module.urllib.request.Request("https://openai.com/news/rss.xml")

      with warnings.catch_warnings(record=True) as caught:
          warnings.simplefilter("always", ResourceWarning)
          try:
              handler.redirect_request(request, None, 302, "Found", {}, "https://127.0.0.1/internal")
          except module.urllib.error.HTTPError:
              pass
          del handler

      resource_warnings = [w for w in caught if issubclass(w.category, ResourceWarning)]
      self.assertEqual(resource_warnings, [])
  ```

  Ensure `warnings` is imported at the top of the test file.

- [ ] **Step 4: Run tests with warnings as errors**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -W error::ResourceWarning -m unittest workflows.erhua-morning-brief.tests.test_morning_brief.ErhuaMorningBriefTests.test_news_feed_redirect_handler_rejects_redirects workflows.erhua-morning-brief.tests.test_morning_brief.ErhuaMorningBriefTests.test_news_feed_redirect_handler_does_not_leak_file_pointer -v
  ```

  Expected: Both tests pass without `ResourceWarning`.

- [ ] **Step 5: Commit**

  ```bash
  git add workflows/erhua-morning-brief/morning_brief.py
  git add workflows/erhua-morning-brief/tests/test_morning_brief.py
  git commit -m "fix(workflows): avoid ResourceWarning in NoNewsFeedRedirect"
  ```

---

## Task 5: Simplify environment-variable test with `patch.dict`

**Files:**

- Modify: `workflows/erhua-morning-brief/tests/test_morning_brief.py`

- [ ] **Step 1: Replace manual env save/restore with `patch.dict`**

  Replace `test_news_llm_args_fall_back_to_shared_llm_env` with:

  ```python
  def test_news_llm_args_fall_back_to_shared_llm_env(self):
      module = load_module()
      with unittest.mock.patch.dict(
          os.environ,
          {
              "QINTOPIA_LLM_BASE_URL": "https://llm.example.test/v1",
              "QINTOPIA_LLM_API_KEY": "shared-key",
              "QINTOPIA_LLM_MODEL": "gpt-5.2",
          },
          clear=False,
      ), unittest.mock.patch.dict(
          os.environ,
          {},
          clear=False,
      ):
          os.environ.pop("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL", None)
          args = module._parse_args()

      self.assertEqual(args.news_llm_base_url, "https://llm.example.test/v1")
      self.assertEqual(args.news_llm_api_key, "shared-key")
      self.assertEqual(args.news_llm_model, "gpt-5.2")
  ```

  Note: `patch.dict` with `clear=False` automatically restores the previous values after
  the `with` block, so explicit cleanup is not required.

- [ ] **Step 2: Run tests**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest workflows.erhua-morning-brief.tests.test_morning_brief.ErhuaMorningBriefTests.test_news_llm_args_fall_back_to_shared_llm_env -v
  ```

  Expected: Test passes.

- [ ] **Step 3: Commit**

  ```bash
  git add workflows/erhua-morning-brief/tests/test_morning_brief.py
  git commit -m "test(workflows): use patch.dict for env fallback test"
  ```

---

## Task 6: Final validation and documentation check

**Files:**

- Read: `workflows/erhua-morning-brief/README.md`
- Modify (if needed): `workflows/erhua-morning-brief/README.md`

- [ ] **Step 1: Review README consistency**

  Confirm README section `## How it works` states the `--news-llm-*` variables fall back
  to shared `QINTOPIA_LLM_*` env vars. If inconsistent, update the paragraph to:

  ```markdown
  RSS fallback English items are translated through the optional news LLM endpoint when
  configured (`QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL` / `_API_KEY` / `_MODEL`,
  falling back to the shared `QINTOPIA_LLM_BASE_URL` / `_API_KEY` / `_MODEL` when the
  brief-specific vars are unset); without any endpoint, English-only RSS items are
  skipped so the community group never receives untranslated English.
  ```

- [ ] **Step 2: Run full unit test suite**

  Run:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/erhua-morning-brief/tests -v
  ```

  Expected: All tests pass.

- [ ] **Step 3: Run repository checks**

  Run:

  ```bash
  pnpm workflows:check
  pnpm check:pr:quick
  pnpm lint:md
  ```

  Expected: All pass.

- [ ] **Step 4: Final commit**

  If README was updated:

  ```bash
  git add workflows/erhua-morning-brief/README.md
  git commit -m "docs(workflows): align README with shared LLM env fallback"
  ```

---

## Self-Review

1. **Spec coverage:**
   - Fix dependency declaration → Task 1
   - Extract env fallback helper → Task 2
   - Extract sidecar action helper → Task 3
   - Fix ResourceWarning → Task 4
   - Simplify env test → Task 5
   - README consistency → Task 6

2. **Placeholder scan:** No TBD, TODO, or vague steps. Each step includes exact code or
   commands.

3. **Type consistency:**
   - `_env_with_fallback(specific_key: str, shared_key: str, default: str = "") -> str`

   <!-- markdownlint-disable-next-line MD013 -->
   - `_run_sidecar_action(command: list[str], *, args: argparse.Namespace, execute_flag: str, apply_flag: str, error_message: str) -> dict[str, Any]`
   - All usages match these signatures.

4. **Test strategy:** Each new helper gets direct unit tests before refactoring callers.
