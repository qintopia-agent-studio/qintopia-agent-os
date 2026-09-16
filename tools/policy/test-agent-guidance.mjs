import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { validateAgentGuidance } from "./agent-guidance.mjs";

const fixture = fileURLToPath(new URL("./fixtures/agent-guidance/", import.meta.url));
function withFixture(run) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "agent-guidance-"));
  try {
    fs.cpSync(fixture, root, { recursive: true });
    for (const entry of fs.readdirSync(root, { recursive: true })) {
      if (entry.endsWith(".fixture"))
        fs.renameSync(path.join(root, entry), path.join(root, entry.slice(0, -8)));
    }
    run(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}
function rewrite(root, file, transform) {
  const target = path.join(root, file);
  fs.writeFileSync(target, transform(fs.readFileSync(target, "utf8")));
}
function rejects(root, prefix) {
  assert.ok(
    validateAgentGuidance(root).some((e) => e.startsWith(prefix + ":")),
    prefix
  );
}

test("scoped entries pass without an audit inventory", () =>
  withFixture((root) => {
    assert.deepEqual(validateAgentGuidance(root), []);
    rewrite(root, "AGENTS.md", (s) => s.replace("Project rules", "Contributor entry"));
    assert.deepEqual(validateAgentGuidance(root), []);
  }));
test("missing scoped entry is rejected", () =>
  withFixture((root) => {
    fs.unlinkSync(path.join(root, "deploy/AGENTS.md"));
    rejects(root, "missing-file");
  }));
test("missing local target and missing anchor are rejected", () =>
  withFixture((root) => {
    rewrite(
      root,
      "AGENTS.md",
      (s) => s + "\n[Absent](absent.md)\n[Anchor](docs/engineering/topic.md#absent)\n"
    );
    rejects(root, "broken-link");
    rejects(root, "missing-anchor");
  }));
test("size budget, scope and unknown indexed command are enforced", () =>
  withFixture((root) => {
    rewrite(
      root,
      "AGENTS.md",
      (s) =>
        s.replace("guidance-scope: .", "guidance-scope: wrong") +
        "\nRun `pnpm absent:check`.\n" +
        "x\n".repeat(9000)
    );
    rejects(root, "size-budget");
    rejects(root, "scope-marker");
    rejects(root, "missing-command");
  }));
test("repository traversal is rejected", () =>
  withFixture((root) => {
    rewrite(root, "AGENTS.md", (s) => s + "\n[Outside](../outside.md)\n");
    rejects(root, "outside-repository");
  }));

test("overlong lines cannot bypass readable entry budgets", () =>
  withFixture((root) => {
    rewrite(root, "AGENTS.md", (s) => s + "\n" + "x".repeat(501));
    rejects(root, "long-line");
  }));
test("routing section is required", () =>
  withFixture((root) => {
    rewrite(root, "docs/engineering/change-routing-index.md", () => "# Routing\n");
    rejects(root, "missing-routing");
  }));
test("linked operating rules also receive link validation", () =>
  withFixture((root) => {
    rewrite(
      root,
      "AGENTS.md",
      (s) => s + "\n[Rules](docs/engineering/topic.md#operating-rules)\n"
    );
    rewrite(
      root,
      "docs/engineering/topic.md",
      (s) => s + "\n## Operating rules\n\n[Missing](absent.md)\n"
    );
    rejects(root, "broken-link");
  }));
