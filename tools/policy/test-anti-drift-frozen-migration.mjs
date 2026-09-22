#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../", import.meta.url));
const checker = path.join(repoRoot, "tools/policy/check-anti-drift.mjs");
const frozen =
  "runtime/postgres/migrations/202609230006_person_stay_building_history.sql";
const repair =
  "runtime/postgres/migrations/202609230007_person_stay_building_history_registration.sql";
const design = "runtime/postgres/docs/data-design/2026-09-23-ontology-audience.md";
const expanded = new Set(["", "runtime", "runtime/postgres", "runtime/postgres/docs"]);
const copied = new Set([
  "runtime/postgres/migrations",
  "runtime/postgres/docs/data-design",
]);

// Run the real checker. Only the migration/design trees are writable copies;
// all unrelated inputs are read through symlinks to the current repository.
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-frozen-migration-"));
  function mirror(relative = "") {
    fs.mkdirSync(path.join(root, relative), { recursive: true });
    for (const entry of fs.readdirSync(path.join(repoRoot, relative), {
      withFileTypes: true,
    })) {
      const child = path.join(relative, entry.name);
      const source = path.join(repoRoot, child);
      const target = path.join(root, child);
      if (expanded.has(child)) mirror(child);
      else if (copied.has(child)) fs.cpSync(source, target, { recursive: true });
      else fs.symlinkSync(source, target, entry.isDirectory() ? "dir" : "file");
    }
  }
  mirror();
  return root;
}

function edit(root, file, transform) {
  const target = path.join(root, file);
  fs.writeFileSync(target, transform(fs.readFileSync(target, "utf8")));
}

const cases = [
  ["unchanged applied migration and additive repair pass", () => {}, null],
  [
    "changed frozen bytes cannot be excused by a new inline design reference",
    (root) => {
      edit(
        root,
        frozen,
        (sql) => `${sql}\n-- Design: docs/data-design/2026-09-23-ontology-audience.md\n`
      );
    },
    /frozen applied migration SHA-384 mismatch/,
  ],
  [
    "missing frozen migration fails",
    (root) => fs.unlinkSync(path.join(root, frozen)),
    /frozen applied migration is missing/,
  ],
  [
    "missing additive repair fails",
    (root) => fs.unlinkSync(path.join(root, repair)),
    /required additive repair .* is missing/,
  ],
  [
    "missing design document fails",
    (root) => fs.unlinkSync(path.join(root, design)),
    /referenced design note 2026-09-23-ontology-audience\.md does not exist/,
  ],
  [
    "repair must register the exact historical migration",
    (root) => {
      edit(root, repair, (sql) =>
        sql.replace(
          "'202609230006_person_stay_building_history.sql'",
          "'202609230008_unrelated.sql'"
        )
      );
    },
    /must register and repair/,
  ],
  [
    "repair must explicitly identify migration 006",
    (root) => {
      edit(root, repair, (sql) =>
        sql.replace('"repairs":"202609230006"', '"repairs":"202609230008"')
      );
    },
    /must register and repair/,
  ],
  [
    "repair must retain this design reference",
    (root) => {
      edit(root, repair, (sql) =>
        sql.replaceAll(
          "docs/data-design/2026-09-23-ontology-audience.md",
          "docs/data-design/2026-09-22-person-memory.md"
        )
      );
    },
    /must register and repair/,
  ],
  [
    "unrelated undocumented migrations still fail",
    (root) => {
      fs.writeFileSync(
        path.join(root, "runtime/postgres/migrations/202609230008_undocumented.sql"),
        "SELECT 1;\n"
      );
    },
    /202609230008_undocumented\.sql: migration must reference at least one docs\/data-design/,
  ],
];

for (const [name, mutate, expected] of cases) {
  const root = fixture();
  try {
    mutate(root);
    const result = spawnSync(process.execPath, [checker], {
      cwd: root,
      encoding: "utf8",
    });
    assert.ifError(result.error);
    const output = `${result.stdout}\n${result.stderr}`;
    if (expected) {
      assert.equal(result.status, 1, `${name}: ${output}`);
      assert.match(output, expected, name);
    } else {
      assert.equal(result.status, 0, `${name}: ${output}`);
      assert.match(output, /Anti-drift policy check passed\./);
    }
    console.log(`PASS ${name}`);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}
console.log(`Frozen migration policy: ${cases.length} checks passed.`);
