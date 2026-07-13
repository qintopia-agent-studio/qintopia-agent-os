# Reports

This directory links internal sync reports and generated reports that are useful for
team onboarding, architecture review, or migration decisions.

## Current References

| Source                                                                             | Disposition | Notes                                                                  |
| ---------------------------------------------------------------------------------- | ----------- | ---------------------------------------------------------------------- |
| `../qintopia-agent-os/docs/reports/agent-os-internal-sync-2026-07-03.html`         | report-ref  | Internal sync HTML for architecture, current state, risks, and roadmap |
| `../qintopia-agent-os/docs/reports/server-agent-runtime-inventory-2026-06-29.md`   | adopt-input | Runtime inventory used by migration planning                           |
| `../qintopia-agent-os/docs/reports/qintopia-agent-os-kb-synthesis-2026-06-28.html` | report-ref  | Knowledge-base synthesis report                                        |
| `2026-07-12-xiaoman-postgres-integration-hardening.md`                             | adopt-input | PostgreSQL integration smoke findings and remediation record           |
| `2026-07-12-xiaoman-group-send-ready-observation.md`                               | adopt-input | Group send-ready production observation gap and remediation record     |
| `2026-07-12-xiaoman-production-release-gap.md`                                     | adopt-input | Release payload and timer-installation gap remediation record          |
| `2026-07-13-xiaoman-downstream-dry-run-report.md`                                  | adopt-input | Production preflight dry-run report mismatch and remediation record    |
| `2026-07-13-aliang-image-provider-runtime-inventory.md`                            | adopt-input | Read-only evidence for the historical OpenAI-compatible Image2 path    |
| `2026-07-13-aliang-image-generation-ci-smoke-fix.md`                               | adopt-input | Capability-count CI smoke remediation record                           |
| `2026-07-13-aliang-image-adapter-local-integration.md`                             | adopt-input | Local pgvector/Docker preflight limitation and CI validation boundary  |
| `2026-07-13-aliang-image-adapter-review-remediation.md`                            | adopt-input | Response-limit and reviewed-artifact immutability remediation record   |
| `2026-07-13-documentation-architecture-consistency-audit.md`                       | adopt-input | Documentation, implementation, and production-state consistency audit  |

## Rules

- Reports are evidence and communication artifacts.
- Stable decisions should be promoted into `docs/architecture/`, `docs/engineering/`,
  `docs/product/`, `docs/operations/`, or package README files.
- HTML reports should be checked with an HTML parser and browser review when modified.
