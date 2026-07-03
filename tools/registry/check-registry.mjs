#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import Ajv2020 from "ajv/dist/2020.js";
import YAML from "yaml";

const repoRoot = process.cwd();

const domainConfig = [
  { domain: "agents", registry: "registry/agents.yaml", manifestNames: ["agent.yaml"] },
  {
    domain: "skills",
    registry: "registry/skills.yaml",
    manifestNames: ["manifest.yaml"],
  },
  {
    domain: "workflows",
    registry: "registry/workflows.yaml",
    manifestNames: ["workflow.yaml"],
  },
  { domain: "mcp", registry: "registry/mcp.yaml", manifestNames: ["manifest.yaml"] },
  {
    domain: "runtime",
    registry: "registry/runtime.yaml",
    manifestNames: ["manifest.yaml"],
  },
  {
    domain: "deploy",
    registry: "registry/deploy.yaml",
    manifestNames: ["manifest.yaml"],
  },
  {
    domain: "deprecated",
    registry: "registry/deprecated.yaml",
    manifestNames: ["manifest.yaml"],
  },
];

const readJson = (relativePath) => {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), "utf8"));
};

const readYaml = (relativePath) => {
  return YAML.parse(fs.readFileSync(path.join(repoRoot, relativePath), "utf8"));
};

const fileExists = (relativePath) => {
  return fs.existsSync(path.join(repoRoot, relativePath));
};

const ajv = new Ajv2020({ allErrors: true, strict: true });
const manifestSchema = readJson("registry/schemas/package-manifest.schema.json");
const indexSchema = readJson("registry/schemas/registry-index.schema.json");
const validateManifest = ajv.compile(manifestSchema);
const validateIndex = ajv.compile(indexSchema);

const errors = [];

const formatErrors = (file, validationErrors) => {
  for (const error of validationErrors ?? []) {
    const location = error.instancePath || "/";
    errors.push(`${file}: ${location} ${error.message}`);
  }
};

const validateYamlWithSchema = (relativePath, validator) => {
  let data;
  try {
    data = readYaml(relativePath);
  } catch (error) {
    errors.push(`${relativePath}: failed to parse YAML: ${error.message}`);
    return null;
  }

  if (!validator(data)) {
    formatErrors(relativePath, validator.errors);
    return data;
  }

  return data;
};

for (const config of domainConfig) {
  const index = validateYamlWithSchema(config.registry, validateIndex);
  if (!index) {
    continue;
  }

  if (index.domain !== config.domain) {
    errors.push(`${config.registry}: domain must be ${config.domain}`);
  }

  for (const entry of index.entries) {
    if (!entry.id.startsWith(`${config.domain}/`)) {
      errors.push(`${config.registry}: ${entry.id} must start with ${config.domain}/`);
    }

    if (!entry.path.startsWith(`${config.domain}/`)) {
      errors.push(
        `${config.registry}: ${entry.path} must start with ${config.domain}/`
      );
    }

    if (!fileExists(entry.manifest)) {
      errors.push(`${config.registry}: missing manifest ${entry.manifest}`);
      continue;
    }

    const manifest = validateYamlWithSchema(entry.manifest, validateManifest);
    if (manifest && manifest.id !== entry.id) {
      errors.push(
        `${entry.manifest}: manifest id ${manifest.id} does not match registry id ${entry.id}`
      );
    }
  }

  for (const manifestName of config.manifestNames) {
    const templatePath = `${config.domain}/_template/${manifestName}`;
    if (fileExists(templatePath)) {
      const manifest = validateYamlWithSchema(templatePath, validateManifest);
      if (manifest && manifest.type !== config.domain.replace(/s$/, "")) {
        const expectedType =
          config.domain === "mcp" ? "mcp" : config.domain.replace(/s$/, "");
        if (manifest.type !== expectedType) {
          errors.push(`${templatePath}: type must be ${expectedType}`);
        }
      }
    }
  }
}

if (errors.length > 0) {
  console.error("Registry check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Registry check passed.");
