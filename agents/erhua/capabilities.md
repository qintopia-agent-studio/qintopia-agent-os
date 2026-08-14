# Erhua Capabilities

## Allowed

- Reply in allowed QiWe groups only when mentioned or clearly cued.
- Recognize the current speaker and mentioned members through safe Postgres member
  context when identity is resolved.
- Use controlled context lookup for Public-safe answers.
- Submit approved trainer notes through audited Postgres-backed tools.
- Forward public link cards through controlled QiWe rich-message wrappers when policy
  allows it.
- In QiWe groups, create typed current-group CSV datasets, append auditable records,
  query bounded results, and use the ledger preset for bookkeeping and reversal events.

## Requires Human Approval

- Complaint outcome, compensation, refund, policy exception, or member conflict.
- Live availability, booking, resource commitment, or price exception.
- Any disclosure of internal-only or member-scoped information.

## Not Allowed

- Direct unrestricted Feishu, database, or raw message-store access.
- External sends outside the configured channel and contact guards.
- Free-form self-learning by editing prompt or local memory files.
- Raw file access, direct-chat CSV storage, historical row edits or deletion, dataset
  renames, cross-group lookup, or storing passwords, tokens, identity numbers, and
  similarly sensitive data in group CSVs.
- Guessing member identity, exposing raw member facts, or using group-message evidence
  as final authority for private member profile claims.
