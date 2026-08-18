# Xiaoman Daily Case Report Rust Migration Plan

Status: PR 6 cutover implemented; Rust pipeline default with Python fallback. Local
validation passed: `cargo fmt --check`,
`cargo clippy --all-targets --all-features --tests -- -D warnings`,
`RUST_MIN_STACK=33554432 cargo test`, `bash -n`, and `python3 -m py_compile` are green.

Scope: migrate the deterministic logic of the Xiaoman daily case report from the Python
workflow package into the Rust sidecar, leaving only HTML-to-JPEG rasterization
(Playwright/Chromium) in Python.

## Why This Exists

The daily case report already runs as a hybrid pipeline:

```text
Hermes cron (jobs.json)
  -> shell wrapper (release-managed worker)
  -> Python workflow package (collect -> analyze -> narrate -> render HTML -> JPEG)
  -> Rust sidecar media upload (governed storage boundary)
  -> Rust sidecar auto-publish work item (governed QiWe send chain)
```

The Rust sidecar already owns the publish control plane (media upload, work-item
creation, QiWe send). The MCP tool `qintopia_daily_case_report_generate`
(`runtime/sidecar/src/daily_case_report_mcp.rs`) already orchestrates on-demand
generation through the same reviewed chain as the scheduled worker. What remains in
Python is the deterministic middle: message collection, topic/person analysis, report
assembly, and HTML rendering. That middle is ~6300 lines of Python that the sidecar
cannot reuse, cannot unit-test through fixtures, and cannot place under the same
release-artifact guarantees as the rest of the control plane.

This plan migrates that deterministic middle into Rust sidecar modules, in reviewable
phases, without changing production behavior until the final cutover PR.

## Completed Prerequisites

- Step 1 (Python package split): merged in `#598`. The former 5184-line
  `daily_case_report.py` god file is now a package: `models.py` (329), `collector.py`
  (635), `analyzer.py` (870), `narrative_generator.py` (345), `report_builder.py`
  (1636), `renderer.py` (1339), `roast_long_image.py` (471), `newspaper_elegant.py`
  (470), plus a 696-line CLI entrypoint.
- Step 2 (MCP orchestration tool): `daily_case_report_mcp.rs` is wired into
  `runtime/sidecar/src/mcp_server.rs` with caller allowlisting, dry-run previews, and
  reuse of the `operations` publish functions.

## Non-Goals

- No behavior change for users before the cutover PR: same report content, same
  templates, same send gates.
- No new language stack, process, or deployment unit. The migration lands inside the
  existing sidecar crate.
- HTML-to-JPEG rasterization stays in Python (Playwright/Chromium). The Rust side
  invokes it as a bounded subprocess, exactly like the MCP tool does today.
- The Hermes cron schedule and the conversation-editable `jobs.json` contract do not
  change.
- The QiWe send path is already Rust (`qiwe_image_send.rs`) and is out of scope.

## Target Shape

```text
runtime/sidecar/src/
  daily_case_report.rs           # orchestration + collection (sqlx)
  daily_case_report_analyze.rs   # topic clustering, person analysis, case detection
  daily_case_report_narrative.rs # LLM roast call via bounded_http
  daily_case_report_render.rs    # report assembly + HTML rendering
  daily_case_report_mcp.rs       # existing: switches from Python render to Rust render
```

The Python package keeps only `renderer.py`'s Playwright rasterization entry, invoked as
a bounded subprocess with sanitized input/output contracts.

## Phase PRs

### PR 1: This Plan

Docs only: this plan. Roadmap and change-routing links are added when the plan is
approved, not in the draft PR.

### PR 2: Rust Collection Preview Command

Deliverables:

- `daily-case-report-collect-preview` sidecar preview command that reads the same
  message/memory sources as `collector.py` through the existing `sqlx` pool, and emits a
  sanitized JSON summary (counts, hashes, byte sizes; never raw message text).
- Golden fixtures under `runtime/sidecar/fixtures/` captured from sanitized Python
  collector output; the Rust output must match the fixture shape field-for-field.
- Disposable-PostgreSQL integration tests behind `postgres-integration-tests`, matching
  existing sidecar test style.

Forbidden: no production reads outside the reviewed preview command, no DB writes, no
user-visible output.

### PR 3: Rust Analysis With Golden-Fixture Parity

Deliverables:

- `daily_case_report_analyze.rs`: topic clustering, person analysis, and case detection
  ported from `analyzer.py`.
- Chinese segmentation via the `jieba-rs` crate, pinned to an exact version like every
  other sidecar dependency. The PR must include a segmentation-parity fixture set
  proving token-level agreement with Python `jieba` on the reviewed corpus; a mismatch
  list is a merge blocker, not a follow-up.
- Pure-function unit tests plus golden analyzer fixtures (sanitized input -> expected
  analysis JSON).

Forbidden: no LLM calls, no network, no DB access in the analyzer module.

### PR 4: Rust Narrative LLM Call

Deliverables:

- `daily_case_report_narrative.rs`: the roast narrative call ported from
  `narrative_generator.py`, routed through the sidecar `bounded_http` client with the
  existing timeout/retry discipline.
- Prompt-construction parity fixtures: for a fixed `ReportData` input, the Rust prompt
  bytes must equal the Python prompt bytes (fixture-compared).
- Failure-classification tests mirroring the Python error taxonomy (timeout, non-JSON,
  empty choices, content-filtered).

Forbidden: no real provider calls in tests; no unbounded HTTP clients.

### PR 5: Rust Report Assembly and HTML Render

Deliverables:

- `daily_case_report_render.rs`: `report_builder.py` assembly plus the HTML templates
  from `roast_long_image.py` / `newspaper_elegant.py` / `renderer.py`'s HTML half,
  ported with byte-exact HTML fixtures per template.
- The module emits the final HTML plus a rasterization request; actual JPEG encoding
  still goes to the Python Playwright subprocess.
- Template fixture corpus: one sanitized `ReportData` per template (roast-long-image,
  newspaper-elegant, v3 legacy) with expected HTML bytes.

Forbidden: no Playwright replacement, no new template features, no typography changes in
the same PR.

### PR 6: Cutover

Prerequisites: PR 2-5 merged; fixture parity evidence reviewed; owner approval recorded.

Deliverables:

- `daily_case_report_mcp.rs` and the scheduled worker switch from the Python render path
  to the Rust modules, keeping the Python subprocess only for HTML-to-JPEG.
- `workflows/xiaoman-daily-case-report/workflow.yaml` status notes updated; the Python
  collector/analyzer/narrative/HTML modules are removed only after one full production
  observation window with the Rust path active.
- Rollback: re-point the worker/MCP tool to the Python render path (kept intact and
  released until the observation window closes).

Validation: one on-demand dry-run and one scheduled-run artifact compared against the
last pre-cutover artifacts (hash of report structure, template id, and JPEG byte
identity for a fixed input).

## Open Design Decisions

- Whether `report_builder.py`'s character-universe logic moves with PR 5 or gets its own
  PR (it has creative-profile coupling with `apply_creative_profile_candidates.py`).
- The sanitized golden-fixture capture procedure: which production-adjacent window is
  safe to record, and how fixtures are scrubbed before entering git.
- Whether the Rust analyzer keeps Python's exact clustering parameters or adopts the
  tuned values currently only in the Python defaults.

Resolve each in the PR that first needs it, not in this planning PR.

## Success Criteria

- The daily case report's deterministic middle runs inside the released Rust sidecar;
  Python remains only for Playwright rasterization.
- Every migrated module has fixture-level parity evidence against its Python
  predecessor.
- The scheduled Hermes cron path and the on-demand MCP path use the same Rust modules,
  with the Python render path available as a rollback until the observation window
  closes.
- No user-visible behavior change at any phase except the cutover PR, whose evidence
  shows artifact parity.
