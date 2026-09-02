# QiWe Restricted Parser Primitives

Updated: 2026-08-14

## Decision

The low-risk QiWe extension lane may not add or modify Rust, Python, JavaScript, build
logic, dependencies, workflows, schemas, permissions, credentials, routing, or send
behavior. A file-path allowlist cannot prove arbitrary source code is only an event
parser.

The initial owner-reviewed infrastructure release provides a fixed parser kernel. After
that release, a programming Agent may extend an event mapping with one append-only
`*.primitive.json` recipe. A recipe is data, not executable source. It can only compose
the kernel operations listed below and must ship with the mapping, sanitized synthetic
fixture, and exact canonical expectation that exercise it.

This is the only automatically releasable interpretation of "add a restricted parser
primitive". A genuinely new algorithm or kernel operation remains an owner-reviewed
runtime change.

## Recipe Contract

Recipes live under:

```text
fixtures/qiwe/event-mappings/_primitives/**/*.primitive.json
```

Version 1 has exactly these fields:

```json
{
  "schema_version": 1,
  "provider": "qiwe",
  "definition_key": "bounded_lowercase_identifier",
  "operations": [],
  "official_sources": ["https://doc.qiweapi.com/doc-123"]
}
```

The fixed kernel accepts at most eight operations from this finite set:

- `base64_utf8`: strict Base64 to bounded UTF-8 text.
- `json_parse`: strict, duplicate-key rejecting, bounded JSON parsing.
- `json_pointer`: select one value with a bounded JSON Pointer.
- `split`: split bounded text with a literal ASCII delimiter and explicit part limit.
- `string_trim`: trim bounded text.
- `array_flatten`: flatten one bounded array level.

Recipes cannot call another recipe. They have no conditionals, loops, regular
expressions, HTTP, SQL, jq/CEL, dynamic code, environment access, destination fields, or
unknown-field passthrough. Existing `opaque_id`, `dedupe`, and `unix_timestamp` mapping
transforms remain outside the recipe so canonical identifier and time normalization
stays explicit in each mapping.

A mapping invokes a recipe only by its immutable repository path:

```json
{
  "op": "restricted_primitive",
  "primitive_ref": "fixtures/qiwe/event-mappings/_primitives/example/v1.primitive.json"
}
```

The build embeds recipes from the release. Runtime validation rejects missing,
duplicated, malformed, or unbounded recipes. Activation still requires an exact
release-registered mapping and successful fixture replay, then follows the existing
per-Space shadow and administrator-confirmation boundary.

## Automatic Lane Contract

The classifier accepts at most one newly added recipe in a candidate. It requires that
the same candidate adds a mapping which references the recipe, at least one synthetic
fixture for that mapping, and exactly one canonical expectation for each fixture.
Existing recipes may be referenced only when the immutable file exists in the audited
head and still satisfies the same schema.

The programming runner may create those declarative files in its disposable worktree. It
still cannot modify source code, register a new kernel operation, install a dependency,
merge, publish, deploy, activate a mapping, or send a message. CI must run the complete
registered fixture suite, not only schema checks, before the fixed label can be consumed
by the separate default-disabled release workflow.

## Owner-Reviewed Boundary

Stop for owner review when the official event cannot be expressed by the mapping DSL
plus this fixed kernel. Adding a kernel operation changes executable parser behavior for
every Space and therefore is not eligible for the append-only automatic lane, even when
the proposed Rust diff appears small.

This means the conversational system can autonomously add many bounded parsing recipes,
but it cannot truthfully promise that every future provider encoding will be supported
without another owner-reviewed infrastructure release.
