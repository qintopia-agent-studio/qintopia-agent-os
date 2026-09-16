import fs from "node:fs";
import path from "node:path";

export const GUIDANCE_SCOPES = [
  ["AGENTS.md", ".", 16 * 1024],
  ["runtime/sidecar/AGENTS.md", "runtime/sidecar", 12 * 1024],
  ["runtime/hermes/AGENTS.md", "runtime/hermes", 12 * 1024],
  ["deploy/AGENTS.md", "deploy", 12 * 1024],
  ["skills/qiwe/AGENTS.md", "skills/qiwe", 12 * 1024],
];
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

/** Validate navigation and entry budgets, not policy semantics or live safety. */
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
  for (const file of ["CLAUDE.md"]) {
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
      // Follow the operating-rule sections explicitly indexed by scoped entries.
      // Do not turn unrelated legacy prose into a new repository-wide link gate.
      if (
        fragment === "operating-rules" &&
        !documents.has(target) &&
        fs.statSync(absolute).isFile()
      ) {
        const fullText = fs.readFileSync(absolute, "utf8");
        const section = fullText.match(/## Operating rules\n([\s\S]*?)(?=\n## |$)/);
        if (section) documents.set(target, section[0]);
      }
      if (
        fragment &&
        fs.statSync(absolute).isFile() &&
        !anchors(fs.readFileSync(absolute, "utf8")).has(fragment)
      ) {
        fail("missing-anchor", `${file}: ${link}`);
      }
    }
  }
  return errors;
}
