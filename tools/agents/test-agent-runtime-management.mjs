#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import Ajv2020 from "ajv/dist/2020.js";
import YAML from "yaml";

const root = process.cwd();
const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "agent-management-"));
// Existing release-boundary fixtures require a 0755 repository root.
fs.chmodSync(fixture, 0o755);
const readYaml = (p) => YAML.parse(fs.readFileSync(path.join(fixture, p), "utf8"));
const writeYaml = (p, v) => fs.writeFileSync(path.join(fixture, p), YAML.stringify(v));
try {
  const files = execFileSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    { cwd: root, encoding: "utf8" }
  )
    .split("\0")
    .filter(Boolean);
  for (const file of new Set(files)) {
    if (
      !fs.existsSync(path.join(root, file)) ||
      !fs.statSync(path.join(root, file)).isFile()
    )
      continue;
    fs.mkdirSync(path.dirname(path.join(fixture, file)), { recursive: true });
    fs.copyFileSync(path.join(root, file), path.join(fixture, file));
  }
  fs.symlinkSync(
    path.join(root, "node_modules"),
    path.join(fixture, "node_modules"),
    "dir"
  );
  const schema = JSON.parse(
    fs.readFileSync(path.join(root, "registry/schemas/package-manifest.schema.json"))
  );
  const validate = new Ajv2020({ strict: false }).compile(schema);
  const base = {
    ...readYaml("agents/anan/agent.yaml"),
    runtime: { management: "unmanaged" },
  };
  assert.equal(validate(base), true, JSON.stringify(validate.errors));
  for (const runtime of [
    { management: "future" },
    { management: "managed" },
    { management: "unmanaged", restart_target: "hermes-probe" },
  ]) {
    assert.equal(validate({ ...base, runtime }), false);
  }
  assert.equal(
    validate({ ...base, type: "skill", id: "skills/probe" }),
    false,
    "non-Agent runtime must retain original shape"
  );
  assert.equal(
    validate({ ...base, id: "agents/default" }),
    false,
    "default unmanaged is not a new exception"
  );
  assert.equal(
    validate({
      ...base,
      runtime: {
        restart_target: "hermes-probe",
        systemd_user_service: "hermes-gateway-probe.service",
      },
    }),
    true
  );
  assert.equal(
    validate({
      ...base,
      runtime: {
        management: "managed",
        restart_target: "hermes-probe",
        systemd_user_service: "hermes-gateway-probe.service",
      },
    }),
    true
  );

  let registry = readYaml("registry/agents.yaml");
  function addAgent(id, runtime) {
    const dir = `agents/${id}`;
    fs.cpSync(path.join(fixture, "agents/anan"), path.join(fixture, dir), {
      recursive: true,
    });
    writeYaml(`${dir}/agent.yaml`, { ...base, id: dir, runtime });
    const template = readYaml(`${dir}/profile.template.yaml`);
    template.agent_id = id;
    writeYaml(`${dir}/profile.template.yaml`, template);
    registry.entries.push({
      id: dir,
      path: dir,
      manifest: `${dir}/agent.yaml`,
      status: "draft",
    });
    writeYaml("registry/agents.yaml", registry);
  }
  addAgent("unmanaged-probe", { management: "unmanaged" });
  const entries = [
    "tools/agents/check-agents.mjs",
    "tools/deploy/check-deploy-runner.mjs",
  ];
  function run(entry) {
    const result = spawnSync(process.execPath, [path.join(fixture, entry)], {
      cwd: fixture,
      encoding: "utf8",
      timeout: 600000,
      maxBuffer: 4 * 1024 * 1024,
    });
    assert.ifError(result.error);
    return { code: result.status, text: result.stdout + result.stderr };
  }
  for (const entry of entries) {
    const result = run(entry);
    assert.equal(result.code, 0, `${entry} legal states:\n${result.text}`);
  }
  // Batch invalid cases so each real checker runs once per state set, not once per example.
  addAgent("unmanaged-fields", {
    management: "unmanaged",
    restart_target: "hermes-unmanaged-fields",
    systemd_user_service: "hermes-gateway-unmanaged-fields.service",
  });
  addAgent("unmanaged-leak", { management: "unmanaged" });
  addAgent("managed-missing", { management: "managed" });
  addAgent("unknown-management", { management: "future" });
  const profiles = readYaml("runtime/hermes/profile-registry.yaml");
  profiles.profiles.push({
    id: "unmanaged-leak",
    agent_manifest: "agents/unmanaged-leak/agent.yaml",
    systemd_user_service: "hermes-gateway-unmanaged-leak.service",
  });
  writeYaml("runtime/hermes/profile-registry.yaml", profiles);
  for (const id of [
    "unmanaged-rule",
    "unmanaged-schema",
    "unmanaged-smoke",
    "unmanaged-payload",
  ]) {
    addAgent(id, { management: "unmanaged" });
  }
  const rules = readYaml("deploy/restart-target-rules.yaml");
  rules.rules.push({
    target: "hermes-erhua",
    paths: ["agents/unmanaged-rule/specific/**"],
    reason: "illegal fixture mapping",
  });
  writeYaml("deploy/restart-target-rules.yaml", rules);
  const schemaPath = path.join(fixture, "deploy/runner/deploy-request.schema.json");
  const poisonedSchema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
  poisonedSchema.properties.restart_targets.items.enum.push("hermes-unmanaged-schema");
  fs.writeFileSync(schemaPath, JSON.stringify(poisonedSchema));
  fs.appendFileSync(
    path.join(fixture, "deploy/runner/smoke-release.sh"),
    "\n# hermes-unmanaged-smoke)\n"
  );
  fs.appendFileSync(
    path.join(fixture, "tools/deploy/build-deploy-bundle.mjs"),
    '\n// "agents/unmanaged-payload/"\n'
  );
  for (const entry of entries) {
    const result = run(entry);
    assert.notEqual(result.code, 0, `${entry} must reject invalid states`);
    for (const expected of [
      "unmanaged-fields/agent.yaml: unmanaged Agent must not declare deployment fields",
      "unmanaged-leak/agent.yaml: unmanaged Agent must not enter managed production lists",
      ...[
        "unmanaged-rule",
        "unmanaged-schema",
        "unmanaged-smoke",
        "unmanaged-payload",
      ].map(
        (id) =>
          `${id}/agent.yaml: unmanaged Agent must not enter managed production lists`
      ),
      "managed-missing/agent.yaml:",
      "unknown-management/agent.yaml: unknown runtime management",
    ]) {
      assert.ok(
        result.text.includes(expected),
        `${entry} missing ${expected}:\n${result.text}`
      );
    }
  }
  console.log(
    "Agent runtime management: schema and both real checker branches passed; existing managed Agents retained."
  );
} finally {
  fs.rmSync(fixture, { recursive: true, force: true });
}
