#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const readYaml = (relativePath) => YAML.parse(readText(relativePath));

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const listFiles = (relativeDir, predicate = () => true) => {
  const root = path.join(repoRoot, relativeDir);
  if (!fs.existsSync(root)) {
    return [];
  }
  return fs
    .readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => path.join(relativeDir, entry.name))
    .filter(predicate)
    .sort();
};

const addError = (message) => {
  errors.push(message);
};

const inventoryFiles = [
  "docs/operations/inventory/local-sources.yaml",
  "docs/operations/inventory/server-sources.yaml",
  "docs/operations/inventory/runtime-assets.yaml",
];

const activeDispositions = new Set(["adopt", "template"]);

for (const file of inventoryFiles) {
  const inventory = readYaml(file);

  if (inventory.inventory_version !== 1) {
    addError(`${file}: inventory_version must be 1`);
  }

  const records = [
    ...(inventory.records ?? []),
    ...(inventory.profiles ?? []),
    ...(inventory.services ?? []),
  ];

  for (const record of records) {
    if (!record.id) {
      addError(`${file}: record is missing id`);
      continue;
    }

    if (!record.disposition) {
      addError(`${file}: ${record.id} is missing disposition`);
    }

    if (record.risk_level && !["low", "medium", "high"].includes(record.risk_level)) {
      addError(`${file}: ${record.id} has invalid risk_level ${record.risk_level}`);
    }

    const lowerText = JSON.stringify(record).toLowerCase();
    if (
      (lowerText.includes("worktool") || lowerText.includes("xiaoqin")) &&
      activeDispositions.has(record.disposition)
    ) {
      addError(
        `${file}: ${record.id} references WorkTool/Xiaoqin but is marked ${record.disposition}; use deprecated, remove, or review-pool`
      );
    }

    if (lowerText.includes("huabaosi") && lowerText.includes("shadow")) {
      if (record.disposition !== "review-pool") {
        addError(
          `${file}: ${record.id} references Huabaosi shadow work but is not review-pool`
        );
      }
      if (!lowerText.includes("owner")) {
        addError(
          `${file}: ${record.id} Huabaosi shadow record must mention owner review`
        );
      }
    }
  }
}

const deployManifest = readYaml("deploy/sidecar/manifest.yaml");
const deployReadme = readText("deploy/sidecar/README.md");
const cutoverPlan = "deploy/sidecar/docs/monorepo-cutover-plan.md";

if (deployManifest.source?.path === "deploy/sidecar/scripts/server-deploy.sh") {
  if (!deployManifest.tags?.includes("legacy-snapshot")) {
    addError(
      "deploy/sidecar/manifest.yaml: server-deploy.sh must be tagged legacy-snapshot"
    );
  }

  if (!/legacy source snapshot/i.test(deployReadme)) {
    addError(
      "deploy/sidecar/README.md: must describe server-deploy.sh as a legacy source snapshot"
    );
  }

  if (!exists(cutoverPlan)) {
    addError(`${cutoverPlan}: required while server-deploy.sh is a legacy snapshot`);
  }
}

const migrationFiles = listFiles("runtime/postgres/migrations", (file) =>
  file.endsWith(".sql")
);
const bootstrapMigrationsWithoutLog = new Set([
  "runtime/postgres/migrations/202606180001_init.sql",
]);
const designDocs = new Set(
  listFiles("runtime/postgres/docs/data-design", (file) => file.endsWith(".md")).map(
    (file) => path.basename(file)
  )
);

const migrationDocReferences = new Set();
for (const file of migrationFiles) {
  const sql = readText(file);
  const references = [...sql.matchAll(/docs\/data-design\/([a-zA-Z0-9._-]+\.md)/g)].map(
    (match) => match[1]
  );

  if (references.length === 0 && !bootstrapMigrationsWithoutLog.has(file)) {
    addError(
      `${file}: migration must reference at least one docs/data-design/*.md note`
    );
  }

  for (const reference of references) {
    migrationDocReferences.add(reference);
    if (!designDocs.has(reference)) {
      addError(`${file}: referenced design note ${reference} does not exist`);
    }
  }
}

for (const file of listFiles(
  "runtime/postgres/docs/data-design",
  (candidate) => candidate.endsWith(".md") && !candidate.endsWith("README.md")
)) {
  const doc = readText(file);
  const migrationReference = doc.match(/Migration:\s*`migrations\/([^`]+\.sql)`/);
  if (migrationReference) {
    const migrationPath = `runtime/postgres/migrations/${migrationReference[1]}`;
    if (!exists(migrationPath)) {
      addError(`${file}: referenced migration ${migrationReference[1]} does not exist`);
    }
  }
}

const domainRegistries = [
  "registry/agents.yaml",
  "registry/skills.yaml",
  "registry/workflows.yaml",
  "registry/mcp.yaml",
  "registry/runtime.yaml",
  "registry/deploy.yaml",
  "registry/deprecated.yaml",
];

const deprecatedRegistry = readYaml("registry/deprecated.yaml");
const deprecatedIds = new Set(
  (deprecatedRegistry.entries ?? []).map((entry) => entry.id)
);
for (const requiredDeprecatedId of [
  "deprecated/worktool",
  "deprecated/worktool-hermes-plugin",
  "deprecated/openclaw",
]) {
  if (!deprecatedIds.has(requiredDeprecatedId)) {
    addError(`registry/deprecated.yaml: missing ${requiredDeprecatedId}`);
  }
}

for (const registryPath of domainRegistries) {
  const registry = readYaml(registryPath);
  for (const entry of registry.entries ?? []) {
    const manifest = readYaml(entry.manifest);
    const sourceText = JSON.stringify(manifest.source ?? {}).toLowerCase();

    if (
      registry.domain !== "deprecated" &&
      (sourceText.includes("worktool") || sourceText.includes("xiaoqin"))
    ) {
      addError(
        `${entry.manifest}: active registry package must not source from WorkTool or Xiaoqin`
      );
    }

    if (
      sourceText.includes("huabaosi") &&
      sourceText.includes("shadow") &&
      manifest.source?.disposition !== "review-pool"
    ) {
      addError(`${entry.manifest}: Huabaosi shadow source must be review-pool`);
    }
  }
}

if (errors.length > 0) {
  console.error("Anti-drift policy check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Anti-drift policy check passed.");
