# Xiaoman Character Universe Daily Report

## Goal

Bring the useful parts of `ftvrpph4yc-eng/wx-cli` into the Xiaoman daily report path
without losing our production read-through of the latest QiWe messages.

The reference project is not a direct runtime service. It is an agent-neutral daily
workspace around `wx-cli`: every run collects one target group's messages, writes
internal digest, roast digest, public-draft Markdown, quote map, review report, ordinary
profiles, roast profiles, and an Obsidian Wiki. Its strength is the long-term content
layer: people, memes, events, relationships, storylines, and timelines grow every day
from second-pass content. The three-month limitation belongs to that project's source
collection assumptions, not to the content model.

## What To Port

1. Character profiles
   - Keep two layers separate:
     - `reply_context`: existing safe operational profile snapshots used by Erhua and
       internal tools.
     - `creative_profile`: a future content layer for character roles, repeated public
       jokes, story functions, reusable scenes, and strict risk boundaries.
   - A creative profile entry must be evidence-anchored by date, message id, topic, or
     generated quote-map reference.
   - A long-term label needs repeated evidence or same-day group uptake. Single-message
     behavior can appear only as a daily character note, not as a stable profile trait.

2. Daily report style
   - Keep the current deterministic latest-24-hour report as the production base.
   - Add public-safe daily character notes from the same latest message window. These
     are role functions such as activity organizer, resource scout, question raiser,
     answerer, or atmosphere keeper.
   - Do not use hidden `member_profile_snapshots` or raw long-term facts in a
     group-bound poster unless a reviewed publish-safe creative layer exists.

3. Wiki and storyline layer
   - Model the reference project's `wiki/people`, `wiki/events`, `wiki/memes`,
     `wiki/topics`, `wiki/storylines`, `wiki/relationships`, and `wiki/timelines` as a
     curated second-pass knowledge layer.
   - In our project, the durable source should be Postgres evidence plus generated daily
     artifacts, not filesystem raw chat archives.
   - A deterministic graph export can be built from curated Wiki-style Markdown or
     future `qintopia_graph` entities and edges. Raw messages must not be fed directly
     into an LLM graph extractor.

## Landed Steps

`workflows/xiaoman-daily-case-report/daily_case_report.py` now emits a `今日人物群像`
section. It uses the current report window's discussion messages for displayed behavior
and can read sanitized role recurrence counts from `qintopia_identity.member_facts`. It
does not display `member_facts.fact_text`, `person_interaction_summaries.summary`, or
hidden `member_profile_snapshots` content.

The same workflow now writes a private Markdown日报 next to the poster render. This
ports the reference project's "poster plus text daily report" shape while keeping the
production worker's temporary-directory cleanup and reviewed image-send boundary.

The workflow also writes a private `character-universe` JSON export next to the render.
It ports the reference project's Wiki/graph structure in a bounded way: people, topics,
events, storyline candidates, and edges are derived from generated daily-report content,
with `raw_messages_included=false` and `profile_fact_text_included=false`.

The production auto-publish worker now forwards only safe counters and schema flags from
that private universe into send-ready metadata. This gives production observation a way
to prove the upgraded character-universe path executed, without retaining raw nodes,
member labels, story labels, or report excerpts.

This keeps the important invariant:

- latest messages remain first-class through the existing Postgres read-through;
- long-term DB maintenance influences ranking and recurrence labels without becoming raw
  public profile text;
- the report gains reference-project-style human texture;
- long-term profile migration stays behind a separate reviewed data and publication
  boundary.

## Proposed Next Steps

1. Add a Postgres-backed `creative_profile` snapshot kind.
   - Use existing `qintopia_identity.member_profile_snapshots` if the schema remains
     sufficient, with `profile_kind='creative_profile'`.
   - Metadata should include `public_surface_allowed=false` by default,
     `evidence_policy=quote_map_or_message_id`, `profile_track=roast_or_public_safe`,
     and `minimum_recurrence`.

2. Add a daily creative-profile worker.
   - Input: latest QiWe messages for the target group plus existing creative profile
     snapshots.
   - Output: candidate creative-profile deltas and a review report.
   - Apply mode should be internal-only until owner-reviewed evidence proves it cannot
     leak sensitive facts or turn one-off comments into permanent labels.

3. Extend the curated character-universe export.
   - The first daily export now covers people, topics, events, storyline candidates, and
     simple edges from the report second pass.
   - Next, add reviewed creative-profile artifacts for memes, relationships, and
     timelines.
   - Default export must continue to exclude raw messages, raw attachments, run logs,
     secrets, and internal-only profile details.

4. Upgrade poster and日报 copy.
   - The current poster can already show today’s character notes plus bounded recurrence
     labels.
   - Rich roast labels, relationship tension, and cross-day jokes still need an explicit
     publish-safe field before they appear in group-bound auto-published posters.

## Boundaries

- Do not import the reference project's manual `wx-cli` collection model as production
  source of truth. Our latest QiWe messages already live in Postgres.
- Do not treat three months as a maximum history window. Use latest 24 hours for daily
  surfaces, rolling 7/14/30-day and all-time evidence for creative-profile recurrence.
- Do not feed raw archives or internal profile text into Graphify or any LLM extractor
  by default.
- Do not auto-publish roast profiles or long-term labels to the group.
