# Xiaoman acceptance contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../plans/active/agents-guidance/README.md) records the baseline
and source locations.

## root-014

<!-- preserved-rule: root-014 -->

- Xiaoman production evidence chain local repository verification:
  `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`

<!-- /preserved-rule: root-014 -->

## root-103

<!-- preserved-rule: root-103 -->

- Real Xiaoman activity production evidence validation:
  `node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>`

<!-- /preserved-rule: root-103 -->
