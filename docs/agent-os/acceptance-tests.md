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

### HK-004 Member Self Identity

- Input: a WeCom direct-chat or group-mentioned speaker asks Erhua "我是谁" or an
  equivalent self-identity question.
- Expected output: when safe member context resolves the speaker, Erhua answers with a
  safe display identity; when identity is missing or ambiguous, Erhua asks for
  clarification and does not guess.
- Required records: `AuditLog` for the member-context read with returned safe fields and
  redactions.
- Approval: not required for safe display identity; required before exposing any
  internal-only member information.

### HK-005 Mentioned Member Identity

- Input: a member asks Erhua who another member is, using a display name, Chinese alias,
  or channel mention such as "小乔是谁".
- Expected output: Erhua uses safe member context before knowledge lookup; if one member
  resolves, it returns only Public-safe or reply-safe context; if multiple members
  match, it asks a clarifying question; if no member resolves, it does not invent an
  identity.
- Required records: `AuditLog` for each member-context read and ambiguity metadata when
  applicable.
- Approval: required before sharing sensitive, internal-only, or conflict-related member
  details.

### HK-006 Discussion History Versus Authority

- Input: a member asks whether the group recently discussed a topic or what people said
  about it.
- Expected output: Erhua uses group-message evidence as discussion/history context only,
  not as final authority for policy, member identity, live availability, or prices.
- Required records: source evidence and risk label.
- Approval: required before turning discussion evidence into an approved public claim.

### HK-007 Useful Degradation

- Input: an otherwise valid public-safe question cannot be answered because the relevant
  source or tool is unavailable.
- Expected output: Erhua gives a useful next step or handoff instead of exposing an
  implementation failure such as "knowledge base unavailable".
- Required records: missing-source or tool-failure reason and optional `FollowUpTask`.
- Approval: required before a human owner sends a final policy or operational answer.

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
