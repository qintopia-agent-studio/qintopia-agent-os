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

`workflows/xiaoman-daily-case-report/daily_case_report.py` now emits public-safe
`今日剧中人` / `人物出场表` surfaces. They use the current report window's discussion
messages for displayed behavior and can read sanitized role recurrence counts from
`qintopia_identity.member_facts`. They do not display `member_facts.fact_text`,
`person_interaction_summaries.summary`, or hidden `member_profile_snapshots` content.

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

The visible poster and private Markdown日报 now use the reference project's narrative
shape instead of the old scoreboard copy. The report opens with `今日主线`, renders
`人物出场表` before evidence quotes, promotes `梗和回调候选`, and keeps `故事线候选` as
the durable Wiki-style bridge. These fields are derived from the latest Postgres message
window plus public-safe role labels and recurrence counts; they do not publish raw
long-term profile text.

The daily workflow now also derives a richer second-pass character layer without adding
a new production source. `CharacterMemory` maps long-term `member_facts` counts into
public-safe recurrence, depth, weight, and callback-seed labels. `CharacterCard` carries
today's arc, meme seed, and topic co-presence relationship hints. The
`character-universe` export now includes candidate `memes`, `callbacks`, and
`relationships` alongside people, topics, events, storylines, and edges. These
relationships are only same-topic group-chat co-presence summaries; they are not private
identity, social, or profile relationships.

The local character-universe readiness checker now guards these second-stage fields and
the regression test that proves same-name people stay separated while meme, callback,
and same-topic relationship candidates remain public-safe.

The private `character-universe` export now also emits `creative_profile_candidates`.
This ports the reference project's long-term people/wiki workflow one step deeper
without opening a new publish surface: candidates carry
`profile_kind='creative_profile'`, daily role, story function, arc, meme/callback seeds,
review policy, and `public_surface_allowed=false`. They are not written to
`member_profile_snapshots`; production worker metadata and completion evidence retain
only candidate counts plus the false public-surface flag.

This keeps the important invariant:

- latest messages remain first-class through the existing Postgres read-through;
- long-term DB maintenance influences ranking and recurrence labels without becoming raw
  public profile text;
- the report gains reference-project-style human texture;
- daily人物弧线、梗回调、同场关系 can be rendered in poster and Markdown from the latest
  message window;
- long-term profile application stays behind a separate reviewed data and publication
  boundary, while daily runs already produce private review candidates.

## Proposed Next Steps

1. Add a reviewed apply path for Postgres-backed `creative_profile` snapshots.
   - Use existing `qintopia_identity.member_profile_snapshots` if the schema remains
     sufficient, with `profile_kind='creative_profile'`.
   - Metadata should include `public_surface_allowed=false` by default,
     `evidence_policy=quote_map_or_message_id`, `profile_track=roast_or_public_safe`,
     and `minimum_recurrence`.
   - Input should be the private `creative_profile_candidates` export plus owner review,
     not raw chat archives.

2. Add a daily creative-profile apply/review worker.
   - Input: latest QiWe messages for the target group plus existing creative profile
     snapshots and the generated candidate export.
   - Output: approved creative-profile deltas and a review report.
   - Apply mode should remain internal-only until owner-reviewed evidence proves it
     cannot leak sensitive facts or turn one-off comments into permanent labels.

3. Extend the curated character-universe export with durable creative-profile inputs.
   - The daily export now covers people, topics, events, meme candidates, callback
     candidates, same-topic co-presence relationships, storyline candidates, and edges
     from the report second pass.
   - Next, add reviewed creative-profile artifacts for cross-day memes, relationships,
     and timelines.
   - Default export must continue to exclude raw messages, raw attachments, run logs,
     secrets, and internal-only profile details.

4. Add owner-reviewed expressive labels for richer cross-day callbacks.
   - The current poster and Markdown now have the narrative slots, but rich roast
     labels, relationship tension, and cross-day jokes still need an explicit
     publish-safe field before they appear in group-bound auto-published posters.

## Boundaries

- Do not import the reference project's manual `wx-cli` collection model as production
  source of truth. Our latest QiWe messages already live in Postgres.
- Do not treat three months as a maximum history window. Use latest 24 hours for daily
  surfaces, rolling 7/14/30-day and all-time evidence for creative-profile recurrence.
- Do not feed raw archives or internal profile text into Graphify or any LLM extractor
  by default.
- Do not auto-publish roast profiles or long-term labels to the group.
