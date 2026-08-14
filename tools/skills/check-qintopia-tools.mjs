#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import YAML from "yaml";

const repoRoot = process.cwd();
const packageRoot = path.join(repoRoot, "skills/qintopia-tools");
const variants = ["erhua", "xiaoman", "wenyuange"];
const requiredRegisteredTools = {
  erhua: [
    "qintopia_wenyuange_lookup",
    "qintopia_weather_lookup",
    "qintopia_erhua_csv_list",
    "qintopia_erhua_csv_create",
    "qintopia_erhua_csv_append",
    "qintopia_erhua_csv_query",
    "qintopia_daily_digest_publish",
  ],
  xiaoman: [
    "qintopia_wenyuange_lookup",
    "qintopia_weather_lookup",
    "qintopia_daily_digest_publish",
    "qintopia_xiaoman_activity_record_get",
    "qintopia_xiaoman_activity_list_by_date",
    "qintopia_xiaoman_activity_announcement_prepare",
    "qintopia_xiaoman_public_reply_rewrite",
    "qintopia_xiaoman_activity_status_update",
    "qintopia_xiaoman_activity_gap_update",
    "qintopia_xiaoman_activity_phase_update",
    "qintopia_xiaoman_activity_handoff_create",
    "qintopia_xiaoman_activity_promotion_review_draft",
    "qintopia_xiaoman_activity_material_summary",
    "qintopia_xiaoman_poster_production_request",
    "qintopia_xiaoman_poster_workflow_status",
  ],
  wenyuange: ["qintopia_wenyuange_lookup"],
};
const errors = [];

const exists = (relativePath) => fs.existsSync(path.join(packageRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const walk = (dir) => {
  const files = [];
  const visit = (absoluteDir) => {
    for (const entry of fs.readdirSync(absoluteDir, { withFileTypes: true })) {
      const absolutePath = path.join(absoluteDir, entry.name);
      if (entry.isDirectory()) {
        visit(absolutePath);
      } else if (entry.isFile()) {
        files.push(path.relative(packageRoot, absolutePath));
      }
    }
  };
  visit(dir);
  return files.sort();
};

for (const variant of variants) {
  const variantPath = `skills/qintopia-tools/variants/${variant}/__init__.py`;

  for (const relativePath of [
    `variants/${variant}/README.md`,
    `variants/${variant}/plugin.yaml`,
    `variants/${variant}/__init__.py`,
    `variants/${variant}/tests/test_qintopia_tools.py`,
  ]) {
    if (!exists(relativePath)) {
      errors.push(`missing ${relativePath}`);
    }
  }

  const variantSource = readText(variantPath);
  const pluginYaml = readText(`skills/qintopia-tools/variants/${variant}/plugin.yaml`);
  let pluginConfig;
  try {
    pluginConfig = YAML.parse(pluginYaml);
  } catch (error) {
    errors.push(`${variant}: plugin.yaml must be valid YAML: ${error.message}`);
    pluginConfig = {};
  }
  const providesTools = pluginConfig.provides_tools;
  if (!Array.isArray(providesTools)) {
    errors.push(`${variant}: plugin.yaml provides_tools must be a YAML list`);
  }
  for (const toolName of requiredRegisteredTools[variant]) {
    if (!providesTools?.includes(toolName)) {
      errors.push(`${variant}: plugin.yaml must list ${toolName}`);
    }
  }
  if (variant === "xiaoman" && !pluginYaml.includes("- pre_gateway_dispatch")) {
    errors.push("xiaoman: plugin.yaml must declare pre_gateway_dispatch");
  }
  if (variant === "xiaoman") {
    for (const envName of [
      "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
      "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
      "QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID",
      "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED",
    ]) {
      if (!pluginYaml.includes(`- name: ${envName}`)) {
        errors.push(`xiaoman: plugin.yaml must declare ${envName}`);
      }
    }
    const variantTests = readText(
      "skills/qintopia-tools/variants/xiaoman/tests/test_qintopia_tools.py"
    );
    for (const fragment of [
      "安全过滤",
      "发送通道本身是通的",
      "执行过程被转成给用户看的回复",
      "保护机制在兜底",
    ]) {
      if (!variantTests.includes(fragment)) {
        errors.push(`xiaoman: public reply regression tests must cover ${fragment}`);
      }
    }
  }
  if (variantSource.includes("_operations_intake_plugin().QINTOPIA")) {
    errors.push(
      `${variant}: operations-intake schemas must not load the delegated package at module import time`
    );
  }
  if (variantSource.includes("Load error: {str(exc)")) {
    errors.push(
      `${variant}: fallback schemas must not expose operations-intake load errors`
    );
  }

  try {
    execFileSync(
      "python3",
      [
        "-c",
        "import pathlib, sys; p=pathlib.Path(sys.argv[1]); compile(p.read_text(), str(p), 'exec')",
        `skills/qintopia-tools/variants/${variant}/__init__.py`,
      ],
      {
        cwd: repoRoot,
        env: {
          ...process.env,
          PYTHONDONTWRITEBYTECODE: "1",
        },
        stdio: ["ignore", "pipe", "pipe"],
      }
    );
  } catch (error) {
    errors.push(
      `py_compile failed for ${variant}: ${error.stderr?.toString() ?? error}`
    );
  }

  try {
    execFileSync(
      "python3",
      [
        "-c",
        `
import importlib.util
import pathlib

plugin_path = pathlib.Path("skills/qintopia-tools/variants/${variant}/__init__.py").resolve()
module_name = "qintopia_tools_${variant}_check"
spec = importlib.util.spec_from_file_location(module_name, plugin_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class Ctx:
    def __init__(self):
        self.names = []
        self.hooks = {}

    def register_tool(self, **kwargs):
        assert kwargs.get("name")
        assert kwargs.get("schema") is not None
        assert callable(kwargs.get("handler"))
        assert kwargs.get("description")
        self.names.append(kwargs["name"])

    def register_hook(self, name, callback):
        assert name
        assert callable(callback)
        self.hooks[name] = callback

ctx = Ctx()
module.register(ctx)
required = set(${JSON.stringify(requiredRegisteredTools[variant])})
missing = sorted(required - set(ctx.names))
assert not missing, missing
if "${variant}" == "xiaoman":
    assert "pre_gateway_dispatch" in ctx.hooks
`,
      ],
      {
        cwd: repoRoot,
        env: {
          ...process.env,
          PYTHONDONTWRITEBYTECODE: "1",
          QINTOPIA_PROFILE_ID: variant,
          QINTOPIA_AGENT_OS_MONOREPO_DIR: repoRoot,
        },
        stdio: ["ignore", "pipe", "pipe"],
      }
    );
  } catch (error) {
    errors.push(
      `import/register smoke failed for ${variant}: ${
        error.stderr?.toString() ?? error
      }`
    );
  }

  let tempRoot;
  try {
    tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), `qintopia-tools-${variant}-`));
    const pluginsDir = path.join(tempRoot, "plugins");
    fs.mkdirSync(path.join(pluginsDir, "qintopia-tools"), { recursive: true });
    fs.copyFileSync(
      path.join(repoRoot, variantPath),
      path.join(pluginsDir, "qintopia-tools", "__init__.py")
    );
    fs.cpSync(
      path.join(repoRoot, "skills/qintopia-weather"),
      path.join(pluginsDir, "qintopia-weather"),
      { recursive: true }
    );
    fs.cpSync(
      path.join(repoRoot, "skills/knowledge-retrieval"),
      path.join(pluginsDir, "knowledge-retrieval"),
      { recursive: true }
    );
    fs.cpSync(
      path.join(repoRoot, "skills/erhua-csv"),
      path.join(pluginsDir, "erhua-csv"),
      { recursive: true }
    );

    execFileSync(
      "python3",
      [
        "-c",
        `
import importlib.util
import json
import pathlib

plugin_path = pathlib.Path("${tempRoot}/plugins/qintopia-tools/__init__.py").resolve()
spec = importlib.util.spec_from_file_location("qintopia_tools_missing_operations_intake", plugin_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class Ctx:
    def __init__(self):
        self.tools = {}
        self.hooks = {}

    def register_tool(self, **kwargs):
        assert kwargs.get("name")
        assert kwargs.get("schema") is not None
        assert callable(kwargs.get("handler"))
        self.tools[kwargs["name"]] = kwargs

    def register_hook(self, name, callback):
        assert name
        assert callable(callback)
        self.hooks[name] = callback

ctx = Ctx()
module.register(ctx)
assert "qintopia_wenyuange_lookup" in ctx.tools
if "qintopia_complaint_intake_create" in ctx.tools:
    schema_text = json.dumps(ctx.tools["qintopia_complaint_intake_create"]["schema"], ensure_ascii=False)
    forbidden = ["${tempRoot}", "Checked paths", "Load error", "/skills/", "/plugins/", "QINTOPIA_AGENT_OS"]
    for item in forbidden:
        assert item not in schema_text, schema_text
    payload = json.loads(ctx.tools["qintopia_complaint_intake_create"]["handler"]({}))
    assert payload["success"] is False
    assert payload["safe_answer_mode"] == "runtime_package_missing"
    assert payload["error"] == "operations-intake runtime package unavailable"
    assert "detail" not in payload
    payload_text = json.dumps(payload, ensure_ascii=False)
    for item in forbidden:
        assert item not in payload_text, payload_text
`,
      ],
      {
        cwd: tempRoot,
        env: {
          ...process.env,
          PYTHONDONTWRITEBYTECODE: "1",
          QINTOPIA_PROFILE_ID: variant,
          QINTOPIA_DIFY_RAW_TOOLS_ENABLE: "",
          QINTOPIA_MESSAGE_STORE_ENABLE: "",
          QINTOPIA_AGENT_OS_SKILLS_DIR: "",
          QINTOPIA_AGENT_OS_RELEASE_DIR: "",
          QINTOPIA_AGENT_OS_MONOREPO_DIR: "",
        },
        stdio: ["ignore", "pipe", "pipe"],
      }
    );
  } catch (error) {
    errors.push(
      `missing operations-intake smoke failed for ${variant}: ${
        error.stderr?.toString() ?? error
      }`
    );
  } finally {
    if (tempRoot) {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    }
  }

  if (variant === "erhua") {
    let missingCsvRoot;
    try {
      missingCsvRoot = fs.mkdtempSync(
        path.join(os.tmpdir(), "qintopia-tools-erhua-missing-csv-")
      );
      const pluginsDir = path.join(missingCsvRoot, "plugins");
      fs.mkdirSync(path.join(pluginsDir, "qintopia-tools"), { recursive: true });
      fs.copyFileSync(
        path.join(repoRoot, variantPath),
        path.join(pluginsDir, "qintopia-tools", "__init__.py")
      );
      fs.cpSync(
        path.join(repoRoot, "skills/qintopia-weather"),
        path.join(pluginsDir, "qintopia-weather"),
        { recursive: true }
      );
      fs.cpSync(
        path.join(repoRoot, "skills/knowledge-retrieval"),
        path.join(pluginsDir, "knowledge-retrieval"),
        { recursive: true }
      );

      execFileSync(
        "python3",
        [
          "-c",
          `
import importlib.util
import json
import pathlib

plugin_path = pathlib.Path("${missingCsvRoot}/plugins/qintopia-tools/__init__.py").resolve()
spec = importlib.util.spec_from_file_location("qintopia_tools_missing_erhua_csv", plugin_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class Ctx:
    def __init__(self):
        self.tools = {}

    def register_tool(self, **kwargs):
        assert kwargs.get("name")
        assert kwargs.get("schema") is not None
        assert callable(kwargs.get("handler"))
        self.tools[kwargs["name"]] = kwargs

    def register_hook(self, name, callback):
        pass

ctx = Ctx()
module.register(ctx)
schema = ctx.tools["qintopia_erhua_csv_list"]["schema"]
assert schema.get("x-qintopia-runtime-disabled") is True, schema
schema_text = json.dumps(schema, ensure_ascii=False)
for item in ["${missingCsvRoot}", "Checked paths", "Load error", "/skills/", "/plugins/", "QINTOPIA_AGENT_OS"]:
    assert item not in schema_text, schema_text
assert ctx.tools["qintopia_erhua_csv_list"]["check_fn"]() is False
payload = json.loads(ctx.tools["qintopia_erhua_csv_list"]["handler"]({}))
assert payload == {
    "success": False,
    "safe_answer_mode": "runtime_package_missing",
    "error": "Erhua CSV runtime package unavailable",
}
`,
        ],
        {
          cwd: missingCsvRoot,
          env: {
            ...process.env,
            PYTHONDONTWRITEBYTECODE: "1",
            QINTOPIA_PROFILE_ID: variant,
            QINTOPIA_DIFY_RAW_TOOLS_ENABLE: "",
            QINTOPIA_MESSAGE_STORE_ENABLE: "",
            QINTOPIA_AGENT_OS_SKILLS_DIR: "",
            QINTOPIA_AGENT_OS_RELEASE_DIR: "",
            QINTOPIA_AGENT_OS_MONOREPO_DIR: "",
          },
          stdio: ["ignore", "pipe", "pipe"],
        }
      );
    } catch (error) {
      errors.push(
        `missing Erhua CSV smoke failed: ${error.stderr?.toString() ?? error}`
      );
    } finally {
      if (missingCsvRoot) {
        fs.rmSync(missingCsvRoot, { recursive: true, force: true });
      }
    }
  }
}

for (const file of walk(packageRoot)) {
  if (file.includes("__pycache__/") || file.endsWith(".pyc")) {
    errors.push(`runtime cache file must not be committed: ${file}`);
  }

  if (file.endsWith(".env") || file.includes("/.env")) {
    errors.push(`env file must not be committed: ${file}`);
  }
}

if (errors.length > 0) {
  console.error("Qintopia tools check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Qintopia tools check passed.");
