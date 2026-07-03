# Acceptance Tests

Updated: 2026-07-03

These are scenario-level acceptance tests for Qintopia Agent OS. They guide package
implementation, fixture replay, smoke checks, and human QA.

## Test Format

Each acceptance test should include:

- scenario
- input
- expected output
- required records
- approval requirement
- production boundary touched

## Erhua And Housekeeper

### HK-001 Public-Safe SOP Answer

- Input: a WeCom group member asks Erhua a common accommodation question covered by an
  approved public-safe SOP or FAQ.
- Expected output: concise group reply, same conversation, safe mention when sender ID
  is stable.
- Required records: `Consultation`, source evidence, confidence, risk label.
- Approval: not required for clearly public-safe standard information.

### HK-002 Missing Source

- Input: a member asks a question without approved source evidence.
- Expected output: Erhua does not invent policy and routes to housekeeper or owner.
- Required records: `Consultation` with missing-source flag and optional `FollowUpTask`.
- Approval: required before final policy answer.

### HK-003 Complaint Or Dissatisfaction

- Input: a member expresses dissatisfaction or complaint intent.
- Expected output: warm acknowledgement, minimum detail collection if safe, no
  resolution promise.
- Required records: `ComplaintCase`, `ServiceCase`, `FollowUpTask`.
- Approval: required for resolution and outbound final response.

## Content And Promotion

### CR-001 Topic Pool From Operations Signals

- Input: operator requests topic candidates from consultations, activities, and recent
  content performance.
- Expected output: topic candidates with source, channel, audience, angle, and priority.
- Required records: `ContentTopic` proposals.
- Approval: required when member stories, internal data, or sensitive context are used.

### CR-002 Draft Requires Human Review

- Input: Agent generates a Xiaohongshu note, WeChat Official Account draft, poster copy,
  or visual artifact.
- Expected output: draft body or artifact reference, source list, risk labels, review
  status.
- Required records: `ContentOutput`, optional approval request.
- Approval: required before external publication.

### CR-003 Published Content Review

- Input: operator provides a published content link and metrics.
- Expected output: review summary, weakness, next topic ideas, reusable FAQ or script
  candidates when appropriate.
- Required records: `ContentPublish`, `ContentMetric`, `ContentReview`.
- Approval: required before updating SOP or publishing follow-up content.

## Engineering And Deployment

### ENG-001 Server Change Through Git

- Input: a production-adjacent change request touches runtime templates, systemd, nginx,
  profile routing, external sends, or secrets.
- Expected output: branch, PR, validation result, smoke plan, rollback note, approved
  commit SHA.
- Required records: PR, changelog entry, package/runbook update when relevant.
- Approval: required before deploy.

### ENG-002 No Server Hot Edit

- Input: collaborator discovers a server-side doc, script, or code problem.
- Expected output: evidence captured read-only, fix implemented in git, deployment
  handled by runbook.
- Required records: source path, disposition, owner, validation command.
- Approval: required for production deploy.
