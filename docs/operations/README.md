# Operations

This directory contains operational evidence, source inventories, and future runbook
inputs. Server documents are summarized here as evidence before they are adopted into
canonical architecture, engineering, package, or deployment docs.

## Documents

- [source-document-inventory.md](source-document-inventory.md): read-only inventory of
  server and local documents reviewed during the documentation organization pass.

## Rules

- Do not edit server docs or code directly.
- Convert deployment evidence into runbooks through reviewed git changes.
- Treat server-side exploration as `review-pool` until owner review.
- Do not copy live secrets, `.env` files, generated caches, raw member profile text, or
  private chat logs into this repository.
