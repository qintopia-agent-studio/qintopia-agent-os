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
   - Use the reference project's daily content workshop as the production content shape:
     ordinary digest, light-roast candidate, public-draft candidate, quote map, review,
     Wiki-style second pass, storyline continuity, and graph-ready counts.
   - Keep Xiaoman's latest Postgres QiWe messages as the production source and daily
     collection window. The old scoreboard/report-card surface is not the target shape.
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

The public poster now keeps activity rhythm and speaker ranking after the main
character/storyline sections, so the first screen reads as a character-universe daily
rather than a statistics dashboard. The private Markdown日报 follows the reference
`digest-template` structure more directly: `今日一句话`, `天气背景`, `主要话题`,
`人物动态`, `待解决问题`, `候选公众号选题`, plus the quote/storyline sections. Weather
is represented as an explicit omitted slot unless a reviewed source exists; the workflow
must not invent it.

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
review policy, and `public_surface_allowed=false`. Each candidate now also carries a
safe `daily_character_note:<node-key>` evidence anchor, recurrence evidence count,
minimum-recurrence gate, upgrade status (`eligible_for_review` vs `daily_note_only`),
and a blocked reason when the signal is only a same-day note. They are not written to
`member_profile_snapshots`; production worker metadata and completion evidence retain
only candidate counts plus the false public-surface flag.

The Wiki topic layer now uses generated case storyline titles as candidate topics when
the titles are real discussion labels. Generic time-bucket fallback cards such as
`早场 10:00 时段` stay out of `wiki/topics`. This keeps the reference project's
people/topics/events/memes/storylines shape present even when token repetition alone is
too weak to surface meaningful hot topics.

This keeps the important invariant:

- latest messages remain first-class through the existing Postgres read-through;
- long-term DB maintenance influences ranking and recurrence labels without becoming raw
  public profile text;
- the report gains reference-project-style human texture;
- daily人物弧线、梗回调、同场关系 can be rendered in poster and Markdown from the latest
  message window;
- long-term profile application stays behind a separate reviewed data and publication
  boundary, while daily runs already produce anchored private review candidates and
  block one-off notes from being treated as stable profile traits.

The workflow now has a narrow reviewed apply path for Postgres-backed `creative_profile`
snapshots: `workflows/xiaoman-daily-case-report/apply_creative_profile_candidates.py`.
It uses `qintopia_identity.member_profile_snapshots` with
`profile_kind='creative_profile'` and
`profile_version='xiaoman-daily-creative-profile-v1'`. It accepts only an owner-reviewed
payload with copied `eligible_for_review` candidates and reviewed `person_id` UUID
mappings. It rejects `daily_note_only`, weak recurrence, guessed display-name
identities, `public_surface_allowed=true`, unsupported fields, raw/private markers, and
missing apply approval. Its report keeps sanitized counts and privacy flags only.

The production trigger is the fixed `production-runtime-one-shot` target
`xiaoman-creative-profile-candidates-apply`. The GitHub workflow accepts only
`release_sha`, the fixed target, the approval phrase, and a 64-hex payload SHA-256. The
deploy runner reads the reviewed payload only from
`/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json`;
it must not accept payload JSON, payload paths, person ids, display names, candidate
text, or raw profile fields from workflow inputs. Production evidence may retain the
reviewed payload SHA-256 and sanitized counts/privacy flags only.

The daily workflow now also reuses already-reviewed `creative_profile` snapshots as a
read-only style/memory layer. The read path is keyed by stable `person_id`, active
`profile_kind='creative_profile'`, and
`profile_version='xiaoman-daily-creative-profile-v1'`; it reads only `safe_reply_hints`
/ `communication_style`, never snapshot `summary`, fact text, raw messages, or private
profile text. These reviewed hints can shape the daily arc, story-function, meme seed,
and callback hint, while today's role label and evidence still come from the latest
Postgres message window. The run manifest records only the boolean
`reviewed_creative_profiles_used`, plus existing privacy flags and counts.

The workflow now also includes
`workflows/xiaoman-daily-case-report/build_creative_profile_review_payload.py`, a
review-payload draft builder for the generated `creative_profile_candidates`. It reads a
private `.character-universe.json` export, keeps only `eligible_for_review` candidates
by default, leaves `person_id` blank for owner-reviewed stable UUID binding, and writes
eligible entries as `pending_review` rather than `approved`. The owner must explicitly
approve accepted candidates before apply. Raw/private markers are rejected, and
`daily_note_only` candidates remain rejected review context unless explicitly included
with `--include-rejected`; they still cannot pass
`apply_creative_profile_candidates.py`.

The private review bundle now also writes `.draft-bundle.json`. This ports the reference
project's ordinary digest / roast digest / public draft / cross-day storyline habit into
our governed path without making those drafts public. The bundle carries owner-review
candidates for ordinary digest structure, light-roast character cards, public-draft
title/opening material, storyline timelines, and 7/14/30-day lookback callbacks.
Production worker metadata and worker-run evidence retain only draft counts and the same
false raw/profile/public-surface privacy flags.

The ordinary digest candidate is no longer only a section list. It now carries
structured `weather_context`, `one_sentence_summary`, `main_topics`, `people_notes`,
`local_life_notes`, `open_questions`, `risk_items`, and `candidate_public_topics`,
matching the reference template while keeping all candidate text private to the review
bundle.

The private `character-universe` export now also emits `creative_universe_candidates`.
This extends the reviewed creative layer beyond person callbacks into cross-day meme
candidates, same-topic relationship label candidates, and timeline-thread candidates.
They are candidate-only review assets with `public_surface_allowed=false`,
`writes_member_profile_snapshots=false`, and raw/profile text excluded; worker metadata
and production evidence retain only the candidate count plus the false public-surface
flag.

Owner-reviewed expressive labels now have an explicit field-level gate. The daily
workflow emits `expressive_label_candidates` for rich roast labels, relationship
tension, and cross-day jokes, but public Markdown/poster rendering can use only labels
already present in reviewed `safe_reply_hints.public_expressive_labels` with
`public_surface_allowed=true` and `review_status=reviewed|approved`. Production metadata
and worker-run evidence retain only expressive-label counts plus the false
`unreviewed_expressive_labels_public_surface_allowed` flag.

## Proposed Next Steps

1. Merge the reviewed PR, then follow the existing release and production Hermes cron
   runbooks. Do not publish or activate until the merged release has passed CI and the
   production observation evidence proves the image-first daily report path.

## Boundaries

- Do not import the reference project's manual `wx-cli` collection model as production
  source of truth. Our latest QiWe messages already live in Postgres.
- Do not treat three months as a maximum history window. Use latest 24 hours for daily
  surfaces, rolling 7/14/30-day and all-time evidence for creative-profile recurrence.
- Do not feed raw archives or internal profile text into Graphify or any LLM extractor
  by default.
- Do not auto-publish roast profiles or long-term labels to the group.
