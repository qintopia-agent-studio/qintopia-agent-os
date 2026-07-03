# Message Capture Data Layer v1

Schema version: `2026-06-18.001`  
Migration: `migrations/202606180001_init.sql`  
Status: applied  
Date: 2026-06-18

## Purpose

This version created the first durable storage layer for QiWe/Hermes message capture.
Its job is to persist inbound webhook facts without affecting 二花's reply path.

The capture sidecar consumes NATS JetStream events and writes Postgres records. It does
not call LLMs, embedding APIs, Feishu, Dify, or Agent tools.

## Scope

Schema: `qintopia_messages`

Tables:

- `raw_events`: raw webhook payloads and duplicate tracking.
- `messages`: normalized message facts.
- `message_mentions`: mention relationships from normalized events.
- `message_embeddings`: placeholder table for later message embeddings.
- `message_processing_jobs`: pending async jobs for embedding, entity extraction, and
  graph projection.
- `dead_letter_events`: invalid or unparseable payload capture.
- `entities`, `message_entities`, `entity_edges`: early message-local graph
  placeholders.

## Compatibility

This version is the foundation for later migrations. The sidecar write path uses
explicit column lists so additive columns in later migrations do not break message
capture.

## Known Gaps

The initial version intentionally left several Agent OS concerns for later migrations:

- No versioned schema change log existed yet.
- No knowledge-source/document/chunk tables existed.
- No person/channel identity model existed.
- No controlled context request/result audit existed.
- `message_embeddings.embedding` was created as `vector(1536)`, which assumes one
  embedding dimension and should be generalized before non-1536 models are used.

These gaps are addressed by `2026-06-24.002`.
