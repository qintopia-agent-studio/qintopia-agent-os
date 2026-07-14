# PR-Agent Review Assistant

This repository uses PR-Agent as an advisory pull request reviewer.

PR-Agent may summarize a PR, run an AI review, and answer slash commands in PR comments.
It does not own merge decisions.

## Scope

PR-Agent is allowed to:

- comment on pull requests;
- summarize the changed files and intent;
- flag possible missing tests, docs, production-boundary notes, and architecture drift;
- respond to slash commands such as `/review`, `/describe`, and `/improve`.
- provide advisory changelog suggestions when a maintainer asks for `/update_changelog`.

PR-Agent is not allowed to:

- merge pull requests;
- replace CODEOWNERS review;
- replace required CI checks;
- approve production deploys;
- own root changelog generation or version cutting;
- decide that an unapproved language, framework, or server-side change is acceptable.

## Required GitHub Configuration

Configure repository-level Actions secrets here:

`Settings -> Secrets and variables -> Actions -> Secrets -> New repository secret`

Required secrets:

- `PR_AGENT_OPENAI_KEY`: API key used by PR-Agent for model calls.
- `PR_AGENT_OPENAI_API_BASE`: OpenAI-compatible API base URL, for example
  `https://api.openai.com/v1` or a compatible gateway URL ending in `/v1`.

Configure repository-level Actions variables here:

`Settings -> Secrets and variables -> Actions -> Variables -> New repository variable`

Required variables:

- `PR_AGENT_MODEL`: primary model name, for example `gpt-5-mini` or the model name
  required by the compatible gateway.

Optional variables:

- `PR_AGENT_CUSTOM_MODEL_MAX_TOKENS`: maximum model context size for custom or
  gateway-routed models. Defaults to `32000` in the workflow.

The workflow uses the built-in `GITHUB_TOKEN` for repository comments and pull request
review output. Do not add personal access tokens unless a future documented decision
requires it.

The workflow sets `PR_AGENT_CONFIG_BRANCH` to the PR source branch so that a PR changing
`.pr_agent.toml` can use its own repository settings before those settings have been
merged to `master`.

## Language

PR-Agent is configured to respond in Simplified Chinese through
`response_language = "zh-CN"` and repository-specific Chinese review instructions.
Static headings from the tool may still appear in English.

## Trigger Model

The workflow runs on:

- pull request open, reopen, ready for review, review requested, and synchronize events;
- PR comments that start with `/`, only when the commenter is an `OWNER`, `MEMBER`, or
  `COLLABORATOR`;
- manual workflow dispatch.

Automatic `/improve` is disabled to reduce noise. Maintainers can still request targeted
suggestions from a PR comment when needed.

Automatic `/describe` is disabled. The completed repository PR template remains in the
GitHub PR description and is author-owned because CI validates its required sections.
Maintainers may explicitly request `/describe`; its output is published as a comment and
must not replace the PR body.

`/update_changelog` is advisory. Routine root `CHANGELOG.md` updates are owned by
Release Please, which derives release entries from merged Conventional Commits and keeps
the release PR current.

The third-party PR-Agent Action is pinned to a full commit SHA. Upgrade it through a
normal PR after reviewing the upstream release.

## Merge Boundary

The merge authority remains:

1. branch protection;
2. required CI checks;
3. CODEOWNERS or owner review;
4. resolved conversations;
5. explicit maintainer merge.

PR-Agent output is evidence for reviewers, not a gate by itself.

Before every merge, inspect the complete review state for the current PR head SHA:

1. Read the full PR Reviewer Guide, not only its check conclusion.
2. Read submitted reviews, conversation comments, and every inline review thread.
3. Fix each security concern and recommended review item, or record a concrete
   disposition explaining why no code change is appropriate.
4. After any new push, repeat the inspection because findings for an older head SHA do
   not approve the replacement code.
5. Wait for replacement CI and review results before merging.

A green CI run, a green PR-Agent check, or an earlier review cannot substitute for this
latest-head review.

## Operating Notes

- Treat PR-Agent findings as review candidates, not facts.
- If PR-Agent suggests a direction that conflicts with `AGENTS.md`,
  `docs/engineering/programming-agent-guardrails.md`, or current roadmap docs, follow
  the repository docs.
- If a PR changes production-adjacent paths, include human-readable validation and
  rollback evidence even when PR-Agent does not ask for it.
