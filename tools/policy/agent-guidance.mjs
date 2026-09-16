import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

export const GUIDANCE_SCOPES = [
  ["AGENTS.md", ".", 16 * 1024],
  ["runtime/sidecar/AGENTS.md", "runtime/sidecar", 12 * 1024],
  ["runtime/hermes/AGENTS.md", "runtime/hermes", 12 * 1024],
  ["deploy/AGENTS.md", "deploy", 12 * 1024],
  ["skills/qiwe/AGENTS.md", "skills/qiwe", 12 * 1024],
];
const inventoryDirectory = "docs/plans/active/agents-guidance";
const digest = (text) =>
  crypto.createHash("sha256").update(text.trim().replace(/\s+/g, " ")).digest("hex");
const withoutFences = (text) =>
  text.replace(/^\s*```[^\n]*\n[\s\S]*?^\s*```\s*$/gm, "");
const links = (text) =>
  [...withoutFences(text).matchAll(/(?<!!)\[[^\]\n]*\]\(([^\s)]+)\)/g)].map(
    (m) => m[1]
  );
const slug = (text) =>
  text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}_\-\s]/gu, "")
    .replace(/\s/g, "-");
function anchors(text) {
  const result = new Set();
  const counts = new Map();
  for (const match of withoutFences(text).matchAll(/^#{1,6}\s+(.+)$/gm)) {
    const base = slug(match[1]);
    const count = counts.get(base) ?? 0;
    result.add(count ? `${base}-${count}` : base);
    counts.set(base, count + 1);
  }
  for (const match of text.matchAll(/\bid=["']([^"']+)["']/g)) result.add(match[1]);
  return result;
}

/** Structural coverage only; does not establish policy equivalence or live safety. */
export function validateAgentGuidance(root, { scopes = GUIDANCE_SCOPES } = {}) {
  root = path.resolve(root);
  const errors = [];
  const documents = new Map();
  const fail = (code, detail) => errors.push(`${code}: ${detail}`);
  function resolve(relative) {
    const absolute = path.resolve(root, relative);
    if (absolute !== root && !absolute.startsWith(root + path.sep)) {
      fail("outside-repository", relative);
      return null;
    }
    return absolute;
  }
  function read(relative) {
    const absolute = resolve(relative);
    if (!absolute) return null;
    try {
      return fs.readFileSync(absolute, "utf8");
    } catch {
      fail("missing-file", relative);
      return null;
    }
  }
  function readJson(relative) {
    const text = read(relative);
    if (text === null) return null;
    try {
      return JSON.parse(text);
    } catch {
      fail("invalid-json", relative);
      return null;
    }
  }
  const pkg = readJson("package.json");
  for (const [file, scope, budget] of scopes) {
    const text = read(file);
    if (text === null) continue;
    documents.set(file, text);
    if (!text.includes(`<!-- guidance-scope: ${scope} -->`)) fail("scope-marker", file);
    if (Buffer.byteLength(text) > budget) fail("size-budget", file);
    if (text.split("\n").some((line) => line.length > 500)) fail("long-line", file);
    checkCommands(file, text);
  }
  function checkCommands(file, text) {
    // Validate explicit indexed pnpm commands, not ordinary prose or copied history.
    for (const inline of text.matchAll(/`([^`\n]+)`/g)) {
      for (const match of inline[1].matchAll(/\bpnpm\s+(?:run\s+)?([\w:-]+)/g)) {
        const command = match[1];
        if (["install", "exec", "test:"].includes(command)) continue;
        if (!Object.hasOwn(pkg?.scripts ?? {}, command))
          fail("missing-command", `${file}: ${command}`);
      }
    }
  }
  const baseline = readJson(`${inventoryDirectory}/baseline.json`);
  const migration = readJson(`${inventoryDirectory}/migration.json`);
  const originals = new Map();
  if (
    baseline?.schemaVersion !== 1 ||
    !/^[a-f0-9]{40}$/.test(baseline?.baseline ?? "") ||
    !Array.isArray(baseline?.files)
  ) {
    fail("baseline-schema", inventoryDirectory);
  } else {
    for (const file of baseline.files) {
      if (!Array.isArray(file.entries) || !/^[a-f0-9]{64}$/.test(file.sha256 ?? "")) {
        fail("baseline-file", file.path);
        continue;
      }
      let lastEnd = 0;
      for (const entry of file.entries) {
        if (
          !/^(root|sidecar)-\d{3}$/.test(entry.id ?? "") ||
          originals.has(entry.id) ||
          entry.source !== file.path ||
          !entry.section ||
          !Number.isInteger(entry.start) ||
          !Number.isInteger(entry.end) ||
          entry.start <= lastEnd ||
          entry.end < entry.start ||
          entry.end > file.lineCount ||
          !/^[a-f0-9]{64}$/.test(entry.sha256 ?? "")
        )
          fail("baseline-entry", entry.id);
        originals.set(entry.id, entry);
        lastEnd = entry.end;
      }
    }
  }
  const mapped = new Set();
  const topicFiles = new Set();
  if (migration?.schemaVersion !== 1 || !Array.isArray(migration?.entries)) {
    fail("migration-schema", inventoryDirectory);
  } else {
    for (const entry of migration.entries) {
      if (mapped.has(entry.id)) fail("duplicate-migration", entry.id);
      mapped.add(entry.id);
      const original = originals.get(entry.id);
      if (!original) {
        fail("unknown-source", entry.id);
        continue;
      }
      if (
        !["retained", "moved", "merged", "pending-review"].includes(
          entry.disposition
        ) ||
        !entry.reason
      ) {
        fail("migration-disposition", entry.id);
      }
      if (typeof entry.target !== "string") {
        fail("missing-target", entry.id);
        continue;
      }
      const [file, anchor] = entry.target.split("#");
      if (!file.endsWith(".md") || !anchor || !/^(root|sidecar)-\d{3}$/.test(anchor)) {
        fail("target-anchor", entry.id);
        continue;
      }
      if (!documents.has(file)) {
        const text = read(file);
        if (text !== null) documents.set(file, text);
      }
      const text = documents.get(file);
      if (text === undefined) continue;
      topicFiles.add(file);
      if (!anchors(text).has(anchor)) fail("missing-anchor", entry.target);
      const start = `<!-- preserved-rule: ${anchor} -->`;
      const end = `<!-- /preserved-rule: ${anchor} -->`;
      if (text.split(start).length !== 2 || text.split(end).length !== 2) {
        fail("rule-marker", entry.target);
        continue;
      }
      const body = text.slice(text.indexOf(start) + start.length, text.indexOf(end));
      if (digest(body) !== original.sha256) fail("changed-rule", entry.id);
    }
  }
  for (const id of originals.keys()) if (!mapped.has(id)) fail("missing-migration", id);

  // Validate the new navigation surface without auditing unrelated legacy prose.
  for (const file of ["CLAUDE.md", `${inventoryDirectory}/README.md`]) {
    const text = read(file);
    if (text !== null) documents.set(file, text);
  }
  const routingFile = "docs/engineering/change-routing-index.md";
  const routing = read(routingFile);
  if (routing !== null) {
    const section = routing.match(
      /## Scoped instruction routing\n([\s\S]*?)(?=\n## |$)/
    );
    if (!section) fail("missing-routing", routingFile);
    else {
      documents.set(routingFile, section[0]);
      checkCommands(routingFile, section[0]);
    }
  }
  const incoming = new Set();
  for (const file of topicFiles) {
    let directory = path.dirname(file);
    let index;
    if (file.startsWith("docs/engineering/")) index = "docs/engineering/README.md";
    else if (file.startsWith("docs/operations/")) index = "docs/operations/README.md";
    else {
      while (directory !== ".") {
        if (fs.existsSync(path.join(root, directory, "README.md"))) {
          index = `${directory}/README.md`;
          break;
        }
        directory = path.dirname(directory);
      }
    }
    if (!index) {
      fail("missing-package-index", file);
      continue;
    }
    if (!documents.has(index)) {
      const text = read(index);
      const section = text?.match(
        /## Agent operating contracts\n([\s\S]*?)(?=\n## |$)/
      );
      if (!section) fail("missing-contract-index", index);
      else documents.set(index, section[0]);
    }
    const indexText = documents.get(index) ?? "";
    if (
      !links(indexText).some(
        (link) =>
          path.posix.normalize(
            path.posix.join(path.posix.dirname(index), link.split("#")[0])
          ) === file
      )
    )
      fail("missing-package-link", `${index}: ${file}`);
  }
  for (const [file, text] of documents) {
    for (const link of links(text)) {
      if (/^(?:https?:|mailto:)/.test(link)) continue;
      const [relative, fragment] = link.split("#");
      let decoded;
      try {
        decoded = decodeURIComponent(relative);
      } catch {
        fail("invalid-link", `${file}: ${link}`);
        continue;
      }
      const target = decoded
        ? path.posix.normalize(path.posix.join(path.posix.dirname(file), decoded))
        : file;
      const absolute = resolve(target);
      if (!absolute || !fs.existsSync(absolute)) {
        fail("broken-link", `${file}: ${link}`);
        continue;
      }
      incoming.add(target);
      if (
        fragment &&
        fs.statSync(absolute).isFile() &&
        !anchors(fs.readFileSync(absolute, "utf8")).has(fragment)
      ) {
        fail("missing-anchor", `${file}: ${link}`);
      }
    }
  }
  for (const file of topicFiles) if (!incoming.has(file)) fail("unindexed-topic", file);
  return errors;
}
