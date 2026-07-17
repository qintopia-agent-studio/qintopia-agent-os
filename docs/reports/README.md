# Reports

- [2026-07-15 Aliang image generation production enablement](2026-07-15-aliang-production-enablement.md)
- [2026-07-15 Huabaosi Feishu artifact mirror production enablement](2026-07-15-huabaosi-feishu-artifact-mirror-production-enablement.md)

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
| `2026-07-13-aliang-image-adapter-claim-header-remediation.md`                      | adopt-input | Header injection and stale-claim remediation record                    |
| `2026-07-13-deploy-template-secret-remediation.md`                                 | adopt-input | Deploy-template secret removal and provider rotation record            |
| `2026-07-13-documentation-architecture-consistency-audit.md`                       | adopt-input | Documentation, implementation, and production-state consistency audit  |
| `2026-07-13-pr-agent-pr-body-overwrite.md`                                         | adopt-input | PR-Agent description overwrite CI remediation record                   |
| `2026-07-13-observation-secret-diagnostic-remediation.md`                          | adopt-input | Observation smoke secret-diagnostic remediation record                 |
| `2026-07-13-github-action-download-service-unavailable.md`                         | adopt-input | GitHub Actions hosted-runner setup failure record                      |
| `2026-07-13-pnpm-registry-signature-unavailable.md`                                | adopt-input | Local pnpm signature-validation interruption and bounded fallback      |
| `2026-07-13-aliang-production-observation-stderr-remediation.md`                   | adopt-input | Aliang production observation stderr leak remediation                  |
| `2026-07-13-xiaoman-evidence-local-postgres-integration.md`                        | adopt-input | Xiaoman evidence local pgvector image-pull limitation                  |
| `2026-07-14-xiaoman-mutation-local-postgres-integration.md`                        | adopt-input | Xiaoman mutation local pgvector image-pull limitation                  |
| `2026-07-14-recursive-workflow-status-local-integration.md`                        | adopt-input | Recursive status local pgvector image-pull limitation                  |
| `2026-07-14-generated-image-approval-integrity.md`                                 | adopt-input | Generated-image approval provenance remediation                        |
| `2026-07-14-xiaoman-group-send-postgres-integration.md`                            | adopt-input | Group send-ready Rust/PostgreSQL integration coverage                  |
| `2026-07-14-release-pr-ci-not-triggered.md`                                        | adopt-input | Bot-generated Release Please PR manual CI remediation                  |
| `2026-07-14-pr-body-edit-ci-cancellation.md`                                       | adopt-input | PR body edit replacement-run cancellation evidence                     |
| `2026-07-14-qiwe-preflight-jpeg-state-drift.md`                                    | adopt-input | QiWe preflight stale PNG state remediation                             |
| `2026-07-14-preflight-diagnostic-fixture-drift.md`                                 | adopt-input | Preflight report fixture drift CI remediation                          |
| `2026-07-14-orbstack-license-proxy-unavailable.md`                                 | adopt-input | Local OrbStack proxy/license integration interruption                  |
| `2026-07-14-qiwe-image-send-state-integration-failure.md`                          | adopt-input | QiWe image-send state CI and reviewer remediation                      |
| `2026-07-14-codex-loopback-test-sandbox.md`                                        | adopt-input | Local fake HTTP test sandbox permission evidence                       |
| `2026-07-14-xiaoman-v027-production-preflight.md`                                  | adopt-input | v0.2.7 production preflight hold and smoke remediation                 |
| `2026-07-14-huabaosi-wecom-busy-ack-leak.md`                                       | adopt-input | Huabaosi WeCom busy-ack and formatting fallback leak                   |
| `2026-07-14-qiwe-callback-shape-evidence.md`                                       | adopt-input | QiWe callback credential-shape evidence boundary                       |
| `2026-07-14-staging-adapter-ci-execution-gap.md`                                   | adopt-input | Staging-only Rust tests compiled but not executed in CI                |
| `2026-07-14-huabaosi-command-entry-staging-gate.md`                                | adopt-input | Huabaosi compile and command-entry staging gate remediation            |
| `2026-07-14-v028-release-tag-drift.md`                                             | adopt-input | v0.2.8 release tag drift deploy failure                                |
| `2026-07-14-group-send-ready-claim-cleanup.md`                                     | adopt-input | Group send-ready complete claim release remediation                    |
| `2026-07-15-huabaosi-wecom-v029-production-observation.md`                         | adopt-input | v0.2.9 Huabaosi WeCom observation and runtime-layout remediation       |
| `2026-07-15-huabaosi-stale-claim-ambiguity.md`                                     | adopt-input | Huabaosi expired image-generation claim remediation                    |
| `2026-07-15-xiaoman-production-path-audit.md`                                      | adopt-input | Xiaoman v0.2.9 production execution and remaining ownership gaps       |
| `2026-07-15-xiaoman-qiwe-callback-ingress-gap.md`                                  | adopt-input | Xiaoman QiWe memory-only callback ingress gap                          |
| `2026-07-15-hermes-core-server-patch-inventory.md`                                 | adopt-input | Hermes core dirty-state classification and WeCom patch extraction      |
| `2026-07-15-v0210-follow-up-deploy.md`                                             | adopt-input | v0.2.10 same-SHA deploy mismatch, recovery, and preflight evidence     |
| `2026-07-15-xiaoman-activity-wrapper-contract-drift.md`                            | adopt-input | Xiaoman plugin and Rust event-signal mutation contract repair          |
| `2026-07-15-qiwe-isolated-staging-verification.md`                                 | adopt-input | QiWe isolated staging local verification and remaining owner gate      |
| `2026-07-15-huabaosi-feishu-artifact-mirror-production-enablement.md`              | adopt-input | Production artifact, timer, activation, rollback, and release boundary |
| `2026-07-16-huabaosi-feishu-storage-production-config.md`                          | adopt-input | Production Feishu storage config gap and activation hold               |
| `2026-07-17-xiaoman-activity-read-through-production-recovery.md`                  | adopt-input | Xiaoman read-through production deploy/config recovery evidence        |

## Templates

| Template                                        | Use                                                                          |
| ----------------------------------------------- | ---------------------------------------------------------------------------- |
| `templates/qiwe-image-send-staging-evidence.md` | Sanitized owner-approved QiWe isolated staging upload/callback evidence note |

## Rules

- Reports are evidence and communication artifacts.
- Stable decisions should be promoted into `docs/architecture/`, `docs/engineering/`,
  `docs/product/`, `docs/operations/`, or package README files.
- HTML reports should be checked with an HTML parser and browser review when modified.
