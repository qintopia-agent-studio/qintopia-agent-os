# Xiaoman Daily Case Report Rust Cleanup Plan

Status: pending observation window. Do not implement until the production observation
criteria below are met and owner-approved.

Scope: remove the deprecated Python daily case-report pipeline that PR #644 cut over to
Rust. The Rust sidecar now owns collection, analysis, narrative, report assembly, and
HTML rendering; only Playwright HTML-to-JPEG rasterization stays in Python.

## Why This Exists

PR #644 (`feat(sidecar): cutover xiaoman daily case report to Rust pipeline`) made the
Rust pipeline the default while keeping the Python full-pipeline path available as an
emergency fallback via `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE=1`.

That fallback is valuable during the first production observation window, but it is not
meant to stay forever. Once the Rust path has run successfully for a reviewed period and
the required production observation evidence exists, the fallback code and the obsolete
Python modules should be removed in a single cleanup PR to reduce attack surface, test
matrix, and confusion about which implementation is authoritative.

## Non-Goals

- No new features, templates, or behavior changes.
- No replacement of the Playwright rasterization step; `rasterize.py` stays.
- No changes to the Hermes cron schedule, systemd units, or QiWe send boundary.
- No removal before the observation window evidence is reviewed.

## Observation Window Criteria

The cleanup PR may be opened only after **all** of the following are true:

1. The release containing PR #644 is deployed to production `release/current`.
2. At least one scheduled daily case-report worker run completed via the Rust path with
   `run=ok` in the Hermes cron log.
3. `Observe Production Runtime` was run with target
   `xiaoman-daily-case-report-worker-run` and returned `status=passed`.
4. The observed worker summary does **not** claim `raw_messages_included=true` or
   `profile_fact_text_included=true`.
5. The produced artifact (report structure, template id, and JPEG bytes for a fixed
   input) matches the last pre-cutover artifact within the parity tolerances established
   by PR #644.
6. The production observation deploy result is retained and referenced in the cleanup PR
   body, per `docs/operations/production-current-status.md` step 8.

If any run during the observation window uses the Python fallback env var, extend the
window until a consecutive reviewed period runs entirely on Rust.

## Files to Delete

### Python workflow modules

All become unreachable once the fallback env var and `workflow_py` path are removed:

- `workflows/xiaoman-daily-case-report/analyzer.py`
- `workflows/xiaoman-daily-case-report/collector.py`
- `workflows/xiaoman-daily-case-report/daily_case_report.py`
- `workflows/xiaoman-daily-case-report/narrative_generator.py`
- `workflows/xiaoman-daily-case-report/newspaper_elegant.py`
- `workflows/xiaoman-daily-case-report/renderer.py`
- `workflows/xiaoman-daily-case-report/report_builder.py`
- `workflows/xiaoman-daily-case-report/roast_long_image.py`

Keep:

- `workflows/xiaoman-daily-case-report/rasterize.py`
- `workflows/xiaoman-daily-case-report/models.py` if still used by `rasterize.py` or
  tests; otherwise delete in the same PR.
- `workflows/xiaoman-daily-case-report/tests/` except tests that only cover deleted
  modules. Update or delete those tests explicitly.

### Rust fallback code

- `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE` env-var checks in
  `runtime/sidecar/src/daily_case_report_mcp.rs`.
- `daily_case_report_mcp_workflow_py` CLI/config option and its uses.
- The Python subprocess fallback branch in `daily_case_report_mcp.rs::generate_report`.

### Deploy script fallback branch

- The `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE=1` branch in
  `deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh`.
- Any helper Python heredocs that exist only to support the old pipeline.

## Validation

Run before opening the PR:

```text
cd runtime/sidecar
cargo fmt --check
cargo clippy --all-targets --all-features --tests -- -D warnings
RUST_MIN_STACK=33554432 cargo test
bash -n deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh
python3 -m py_compile workflows/xiaoman-daily-case-report/rasterize.py
pnpm check:pr:auto
```

Expected results:

- All Rust tests pass.
- Worker script parses successfully.
- `rasterize.py` parses successfully.
- No references remain to deleted Python modules from Rust, shell, YAML, or other Python
  files.

## Production Boundary

- Does not touch production servers directly.
- Does not enable new external sends.
- Does not change QiWe, Feishu, Postgres, or MCP caller contracts.
- Removes dead code only; the live Rust path behavior is unchanged.

## Rollback / Decommission

After this cleanup PR is merged and released, reverting to the Python pipeline requires
a code revert and a new Release. The env-var fallback no longer exists. Document the
last Release SHA that still contains the Python fallback in the PR body so operators
know the rollback boundary.

## Success Criteria

- The daily case-report pipeline runs exclusively through Rust modules plus the bounded
  `rasterize.py` Playwright subprocess.
- No deleted Python module is referenced from Rust, shell, YAML, or remaining Python
  code.
- All existing tests pass; no test references deleted modules without an explicit
  decision.
- Production observation evidence from the Rust path is retained and linked in the
  cleanup PR.

## Related Documents

- `docs/plans/active/xiaoman-daily-case-report-rust-migration.md`
- `docs/operations/production-runtime-observation-runbook.md`
- `docs/operations/production-current-status.md`
