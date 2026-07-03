# Erhua Training Memory V1

Status: additive schema and MCP write path.

## Goal

Allow approved Erhua trainers to submit controlled memory through QiWe/Hermes without
enabling production Erhua's generic Hermes `memory`, `file`, or terminal toolsets.

Dynamic memory is stored in Postgres and read back through filtered context MCP
responses. `SOUL.md` and profile config remain the source of truth for core personality
and safety boundaries.

## Tables

- `qintopia_identity.erhua_training_notes`: audited trainer submissions with source
  sender/chat/message, target member fields, training type, status, risk level, and
  applied artifact pointers.
- `qintopia_identity.erhua_persona_overlays`: reviewed global Erhua style overlays.
  Active overlays supplement `SOUL.md` but must not override safety boundaries.

Training statuses are fixed to `pending`, `active`, `rejected`, and `revoked`. Training
types are fixed to `member_preference`, `member_fact`, `reply_example`, and
`persona_rule`.

## Behavior

The MCP tool `qintopia_erhua_training_note_submit` validates the caller profile and
`QINTOPIA_ERHUA_TRAINER_USER_IDS` before writing. Low-risk member preferences and member
facts can become active immediately. Low-risk global persona rules from trainer direct
chats can also become active immediately for v1 trainer ergonomics. Persona rules
submitted from group context remain pending by default. Sensitive, unsafe, or
boundary-overriding training is rejected.

`qintopia_answer_context_prepare` returns only active, filtered training guidance. It
does not expose raw training text, raw messages, hidden profiles, or rejected/pending
notes.

## Rollback

The migration is additive. To disable the feature at runtime, unset
`QINTOPIA_ERHUA_TRAINER_USER_IDS` or remove Erhua's ability to call the training MCP
tool. Existing training notes remain auditable and can be revoked by setting
`status='revoked'` and `revoked_at=now()`.
