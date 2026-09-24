import { spawn, spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import Ajv2020 from "ajv/dist/2020.js";

export const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../.."
);
export const testingRoot = path.join(repoRoot, ".local-testing");
export const catalogPath = path.join(repoRoot, "tools/testing/catalog.json");
export const catalogSchemaPath = path.join(
  repoRoot,
  "tools/testing/catalog.schema.json"
);
export const requirementsPath = path.join(repoRoot, "tools/testing/requirements.lock");
export const composePath = path.join(repoRoot, "tools/testing/compose.yml");
export const venvPath = path.join(testingRoot, "venv");
export const venvPython = path.join(
  venvPath,
  process.platform === "win32" ? "Scripts/python.exe" : "bin/python"
);

export const EXIT_CODES = Object.freeze({
  ok: 0,
  assertion: 1,
  infrastructure: 2,
  interrupted: 130,
});

export class HarnessError extends Error {
  constructor(message, code = EXIT_CODES.infrastructure) {
    super(message);
    this.name = "HarnessError";
    this.code = code;
  }
}

const MAX_OUTPUT = 4 * 1024 * 1024;
const passthroughEnv = [
  "PATH",
  "HOME",
  "USER",
  "TMPDIR",
  "TEMP",
  "TMP",
  "LANG",
  "LC_ALL",
  "RUSTUP_HOME",
  "CARGO_HOME",
  "RUSTUP_TOOLCHAIN",
  "JAVA_HOME",
  "DOCKER_HOST",
  "DOCKER_CONTEXT",
  "SYSTEMROOT",
  "WINDIR",
];

let interrupted = false;
let activeChild = null;
let activeStop = null;

function killTree(child, signal = "SIGTERM") {
  if (!child?.pid) return;
  try {
    if (process.platform === "win32") child.kill(signal);
    else process.kill(-child.pid, signal);
  } catch {
    try {
      child.kill(signal);
    } catch {
      // The child can already have exited.
    }
  }
}

process.on("SIGINT", () => {
  interrupted = true;
  if (activeStop) activeStop();
  else killTree(activeChild);
});
process.on("SIGTERM", () => {
  interrupted = true;
  if (activeStop) activeStop();
  else killTree(activeChild);
});

export function wasInterrupted() {
  return interrupted;
}

export function safeSlug(value) {
  return (
    String(value)
      .replace(/[^a-zA-Z0-9._-]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 96) || "item"
  );
}

export function makeRunId() {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/g, "")
    .slice(0, 14);
  return `${stamp}-${crypto.randomBytes(5).toString("hex")}`;
}

export function ensureDir(directory) {
  fs.mkdirSync(directory, { recursive: true });
  return directory;
}

export function writeJson(file, value) {
  ensureDir(path.dirname(file));
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

export function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch (error) {
    throw new HarnessError(
      `${path.relative(repoRoot, file)}: invalid JSON (${error.message})`
    );
  }
}

function formatAjvErrors(errors) {
  return (errors ?? [])
    .map((error) => `${error.instancePath || "/"} ${error.message}`)
    .join("; ");
}

function assertRelativePath(relativePath, field) {
  if (
    typeof relativePath !== "string" ||
    path.isAbsolute(relativePath) ||
    relativePath.split(/[\\/]/).includes("..")
  ) {
    throw new HarnessError(`${field} must be a repository-relative safe path`);
  }
  const absolute = path.resolve(repoRoot, relativePath);
  if (absolute !== repoRoot && !absolute.startsWith(`${repoRoot}${path.sep}`)) {
    throw new HarnessError(`${field} escapes the repository`);
  }
  return absolute;
}

export function validateCatalog(
  catalog = readJson(catalogPath),
  { requireTargets = true } = {}
) {
  const schema = readJson(catalogSchemaPath);
  const ajv = new Ajv2020({ allErrors: true, strict: true });
  const validate = ajv.compile(schema);
  if (!validate(catalog)) {
    throw new HarnessError(
      `catalog schema validation failed: ${formatAjvErrors(validate.errors)}`
    );
  }

  const featureIds = new Set();
  const scenarioIds = new Set();
  const lookupKeys = new Map();
  const gaps = new Set();
  for (const gap of catalog.gaps) {
    if (gaps.has(gap.id)) throw new HarnessError(`duplicate catalog gap id: ${gap.id}`);
    gaps.add(gap.id);
    assertRelativePath(gap.path, `gap ${gap.id}.path`);
  }

  for (const business of catalog.businesses) {
    if (featureIds.has(business.id))
      throw new HarnessError(`duplicate feature id: ${business.id}`);
    featureIds.add(business.id);
    const featureKeys = [
      ...new Set([business.id, ...business.aliases].map((x) => x.toLowerCase())),
    ];
    for (const key of featureKeys) {
      const normalized = key.toLocaleLowerCase();
      if (lookupKeys.has(normalized)) {
        throw new HarnessError(`catalog alias collides: ${key}`);
      }
      lookupKeys.set(normalized, { type: "feature", feature: business });
    }

    for (const scenario of business.scenarios) {
      if (scenarioIds.has(scenario.id)) {
        throw new HarnessError(`duplicate scenario id: ${scenario.id}`);
      }
      scenarioIds.add(scenario.id);
      for (const key of [
        ...new Set([scenario.id, ...scenario.aliases].map((x) => x.toLowerCase())),
      ]) {
        const normalized = key.toLocaleLowerCase();
        if (lookupKeys.has(normalized)) {
          throw new HarnessError(`catalog alias collides: ${key}`);
        }
        lookupKeys.set(normalized, { type: "scenario", feature: business, scenario });
      }

      const targetPath = assertRelativePath(
        scenario.target.path,
        `scenario ${scenario.id}.target.path`
      );
      if (requireTargets && !fs.existsSync(targetPath)) {
        throw new HarnessError(
          `scenario ${scenario.id}: target does not exist: ${scenario.target.path}`
        );
      }
      if (scenario.target.manifest) {
        const manifestPath = assertRelativePath(
          scenario.target.manifest,
          `scenario ${scenario.id}.target.manifest`
        );
        if (requireTargets && !fs.existsSync(manifestPath)) {
          throw new HarnessError(
            `scenario ${scenario.id}: manifest does not exist: ${scenario.target.manifest}`
          );
        }
      }
      if (scenario.target.cwd) {
        const directory = assertRelativePath(scenario.target.cwd, "target.cwd");
        if (scenario.executor !== "unittest" || !fs.statSync(directory).isDirectory())
          throw new HarnessError(
            "target.cwd is supported only for existing unittest package directories"
          );
      }
      for (const entry of scenario.target.pythonpath ?? []) {
        const directory = assertRelativePath(
          entry,
          `scenario ${scenario.id}.target.pythonpath`
        );
        if (requireTargets && !fs.statSync(directory).isDirectory())
          throw new HarnessError(`Python import directory is missing: ${entry}`);
      }
      const strategy = scenario.target.count_strategy;
      const expectedStrategy = {
        pytest: "junit",
        unittest: "unittest",
        cargo: "cargo",
        node: "node_test",
      }[scenario.executor];
      if (expectedStrategy && strategy !== expectedStrategy) {
        throw new HarnessError(
          `scenario ${scenario.id}: count_strategy must be ${expectedStrategy}`
        );
      }
      if (scenario.executor === "cargo" && !scenario.target.manifest) {
        throw new HarnessError(
          `scenario ${scenario.id}: cargo target needs a manifest`
        );
      }
      if (
        scenario.executor === "cargo" &&
        (!scenario.target.filter || !scenario.target.args?.includes("--exact"))
      ) {
        throw new HarnessError(
          `scenario ${scenario.id}: cargo target needs an exact filter`
        );
      }
    }
  }

  return { catalog, featureIds, scenarioIds, lookupKeys };
}

export function flattenScenarios(catalog) {
  return catalog.businesses.flatMap((feature) =>
    feature.scenarios.map((scenario) => ({ feature, scenario }))
  );
}

export function selectScenarios(catalog, { feature, scenario } = {}) {
  validateCatalog(catalog);
  if (feature && scenario) {
    throw new HarnessError("choose only one of --feature or --scenario");
  }
  const all = flattenScenarios(catalog);
  if (!feature && !scenario) return all;
  const needle = String(feature ?? scenario).toLocaleLowerCase();
  if (feature) {
    const selected = all.filter(({ feature: item }) =>
      [item.id, ...item.aliases].some((key) => key.toLocaleLowerCase() === needle)
    );
    if (selected.length === 0) {
      throw new HarnessError(`unknown feature: ${feature}`);
    }
    return selected;
  }
  const selected = all.filter(({ scenario: item }) =>
    [item.id, ...item.aliases].some((key) => key.toLocaleLowerCase() === needle)
  );
  if (selected.length === 0) throw new HarnessError(`unknown scenario: ${scenario}`);
  return selected;
}

export function safeEnvironment(extra = {}) {
  const env = {};
  for (const key of passthroughEnv) {
    if (process.env[key] !== undefined) env[key] = process.env[key];
  }
  env.PYTHONDONTWRITEBYTECODE = "1";
  env.PYTHONUNBUFFERED = "1";
  env.RUST_MIN_STACK = "33554432";
  env.PYTEST_DISABLE_PLUGIN_AUTOLOAD = "1";
  for (const [key, value] of Object.entries(extra)) {
    if (value !== undefined && value !== null) env[key] = String(value);
  }
  return env;
}

function sanitizeText(value) {
  return String(value)
    .replace(/(postgres(?:ql)?:\/\/)([^\s/]+):([^\s@]+)@/gi, "$1[local-credentials]@")
    .replace(
      /(api[_-]?key|token|secret|password|authorization)(\s*[=:]\s*)[^\s,]+/gi,
      "$1$2[redacted]"
    )
    .replace(/Bearer\s+[A-Za-z0-9._~+/=-]+/gi, "Bearer [redacted]");
}

export { sanitizeText };

export function runProcess(
  argv,
  {
    env = safeEnvironment(),
    cwd = repoRoot,
    timeoutMs = 120_000,
    display = true,
    outputLimit = MAX_OUTPUT,
  } = {}
) {
  if (
    !Array.isArray(argv) ||
    argv.length === 0 ||
    argv.some((part) => typeof part !== "string")
  ) {
    throw new HarnessError("process argv must be a non-empty string array");
  }
  const [command, ...args] = argv;
  if (display) process.stdout.write(`\n$ ${[command, ...args].join(" ")}\n`);
  return new Promise((resolve) => {
    const started = Date.now();
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    let outputTooLarge = false;
    let settled = false;
    let timer;
    let killTimer;
    const stop = () => {
      killTree(child);
      killTimer ??= setTimeout(() => killTree(child, "SIGKILL"), 1500);
      killTimer.unref();
    };
    const child = spawn(command, args, {
      cwd,
      env,
      detached: process.platform !== "win32",
      stdio: ["ignore", "pipe", "pipe"],
    });
    activeChild = child;
    activeStop = stop;
    const append = (name, chunk) => {
      const text = chunk.toString();
      if (name === "stdout") stdout += text;
      else stderr += text;
      if (name === "stdout" && stdout.length > outputLimit) outputTooLarge = true;
      if (name === "stderr" && stderr.length > outputLimit) outputTooLarge = true;
      stdout = stdout.slice(0, outputLimit);
      stderr = stderr.slice(0, outputLimit);
      if (display && !outputTooLarge) process.stdout.write(sanitizeText(text));
      if (outputTooLarge && !settled) {
        stderr += "\n[harness] child output exceeded the safety limit\n";
        stop();
      }
    };
    child.stdout.on("data", (chunk) => append("stdout", chunk));
    child.stderr.on("data", (chunk) => append("stderr", chunk));
    child.on("error", (error) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      if (killTimer) clearTimeout(killTimer);
      activeChild = null;
      activeStop = null;
      resolve({
        argv,
        code: null,
        signal: null,
        stdout,
        stderr: `${stderr}${error.message}`,
        durationMs: Date.now() - started,
        timedOut,
        outputTooLarge,
        interrupted,
        launchError: error.message,
      });
    });
    child.on("close", (code, signal) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      if (killTimer) clearTimeout(killTimer);
      activeChild = null;
      activeStop = null;
      resolve({
        argv,
        code,
        signal,
        stdout,
        stderr,
        durationMs: Date.now() - started,
        timedOut,
        outputTooLarge,
        interrupted,
        launchError: null,
      });
    });
    if (timeoutMs > 0) {
      timer = setTimeout(() => {
        timedOut = true;
        stop();
      }, timeoutMs);
    }
  });
}

export function commandExists(command) {
  const result = spawnSync(command, ["--version"], {
    cwd: repoRoot,
    env: safeEnvironment(),
    stdio: ["ignore", "pipe", "pipe"],
    encoding: "utf8",
    timeout: 15_000,
  });
  return result.status === 0;
}

export function pythonEnvironment(extra = {}) {
  return safeEnvironment({
    ...extra,
    PATH: `${path.dirname(venvPython)}${path.delimiter}${process.env.PATH ?? ""}`,
  });
}

export function runCount(result, strategy, junitFile) {
  const output = `${result.stdout}\n${result.stderr}`;
  if (strategy === "unittest") {
    const ran = output.match(/Ran\s+(\d+)\s+tests?/i);
    if (!ran) return null;
    const skipped = Number(output.match(/skipped\s*=\s*(\d+)/i)?.[1] ?? 0);
    const failed = Number(output.match(/failures=(\d+)/)?.[1] ?? 0);
    const errors = Number(output.match(/errors=(\d+)/)?.[1] ?? 0);
    const total = Number(ran[1]);
    return { total, executed: Math.max(0, total - skipped), skipped, failed, errors };
  }
  if (strategy === "cargo") {
    const matches = [
      ...output.matchAll(
        /test result:\s+\w+\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored;/gi
      ),
    ];
    if (matches.length === 0) return null;
    const totals = matches.reduce(
      (acc, match) => ({
        total: acc.total + Number(match[1]) + Number(match[2]) + Number(match[3]),
        executed: acc.executed + Number(match[1]) + Number(match[2]),
        skipped: acc.skipped + Number(match[3]),
        failed: acc.failed + Number(match[2]),
      }),
      { total: 0, executed: 0, skipped: 0, failed: 0 }
    );
    return totals;
  }
  if (strategy === "node_test") {
    const match = output.match(/(?:#|ℹ)\s*tests?\s+(\d+)/i);
    if (!match) return null;
    const total = Number(match[1]);
    const skipped = Number(output.match(/# skipped (\d+)/)?.[1] ?? 0);
    return {
      total,
      executed: total - skipped,
      skipped,
      failed: Number(output.match(/# fail (\d+)/)?.[1] ?? 0),
    };
  }
  if (!junitFile || !fs.existsSync(junitFile)) return null;
  const parsed = spawnSync(
    venvPython,
    [
      "-c",
      "import json,sys,xml.etree.ElementTree as E; r=E.parse(sys.argv[1]); cases=r.findall('.//testcase'); print(json.dumps({'total':len(cases),'executed':sum(c.find('skipped') is None for c in cases),'skipped':sum(c.find('skipped') is not None for c in cases),'failed':sum(c.find('failure') is not None for c in cases),'errors':sum(c.find('error') is not None for c in cases)}))",
      junitFile,
    ],
    { env: safeEnvironment(), encoding: "utf8", timeout: 15000 }
  );
  if (parsed.status !== 0) return null;
  return JSON.parse(parsed.stdout);
}

function parseDatabaseUrl(databaseUrl) {
  let parsed;
  try {
    parsed = new URL(databaseUrl);
  } catch {
    throw new HarnessError("generated database URL is invalid");
  }
  if (!["postgres:", "postgresql:"].includes(parsed.protocol)) {
    throw new HarnessError("test database URL must use postgres://");
  }
  if (parsed.hostname !== "127.0.0.1" || parsed.pathname !== "/qintopia_test") {
    throw new HarnessError("test database URL must target 127.0.0.1/qintopia_test");
  }
  return parsed;
}

export function databaseSpec({ runId, runDir }) {
  if (!fs.existsSync(composePath))
    throw new HarnessError("tools/testing/compose.yml is missing");
  const project = `qintopia-local-${safeSlug(runId)}`;
  const volume = `qintopia-local-testing-${safeSlug(runId)}`;
  const composeEnv = safeEnvironment({
    QINTOPIA_TEST_RUN_ID: runId,
    QINTOPIA_TEST_VOLUME_NAME: volume,
  });
  const base = ["docker", "compose", "--project-name", project, "--file", composePath];
  return { project, volume, composeEnv, base, runDir, runId };
}

export async function startDatabase(database) {
  const { base, composeEnv, runId, runDir } = database;
  const up = await runProcess([...base, "up", "--detach", "--wait", "postgres"], {
    env: composeEnv,
    timeoutMs: 180_000,
  });
  if (up.code !== 0) throw new HarnessError("PostgreSQL container failed to start");
  const portResult = await runProcess([...base, "port", "postgres", "5432"], {
    env: composeEnv,
    timeoutMs: 15_000,
  });
  const port = Number(portResult.stdout.trim().match(/:(\d+)\s*$/m)?.[1] ?? 0);
  if (portResult.code !== 0 || !Number.isInteger(port) || port < 1 || port > 65535) {
    throw new HarnessError("could not resolve the PostgreSQL loopback port");
  }
  const databaseUrl = `postgres://postgres:postgres@127.0.0.1:${port}/qintopia_test?sslmode=disable`;
  parseDatabaseUrl(databaseUrl);
  const context = {
    run_id: runId,
    run_dir: runDir,
    database_name: "qintopia_test",
    database_host: "127.0.0.1",
    database_port: port,
    compose_project: database.project,
    compose_volume: database.volume,
  };
  writeJson(path.join(runDir, "context.json"), context);
  Object.assign(database, { databaseUrl, port, context });
  return database;
}

export async function migrateDatabase(database, runContext) {
  const env = pythonEnvironment({
    QINTOPIA_SIDECAR_DATABASE_URL: database.databaseUrl,
    QINTOPIA_SIDECAR_MIGRATIONS_DIR: path.join(repoRoot, "runtime/postgres/migrations"),
    QINTOPIA_TEST_MODE: "1",
    QINTOPIA_TEST_RUN_ID: runContext.runId,
    QINTOPIA_TEST_RUN_DIR: runContext.runDir,
    QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE: "1",
  });
  const result = await runProcess(
    [
      path.join(repoRoot, "runtime/sidecar/target/debug/qintopia-message-sidecar"),
      "migrate",
    ],
    { env, cwd: runContext.runDir, timeoutMs: 900_000 }
  );
  if (result.code !== 0) throw new HarnessError("PostgreSQL migration failed");
  await databaseSql(
    database,
    "qintopia_test",
    `CREATE TABLE public.harness_run (run_id text NOT NULL); INSERT INTO public.harness_run VALUES ('${runContext.runId}');`
  );
  return result;
}

export async function databaseSql(database, name, sql) {
  const result = await runProcess(
    [
      ...database.base,
      "exec",
      "-T",
      "postgres",
      "psql",
      "-U",
      "postgres",
      "-d",
      name,
      "-v",
      "ON_ERROR_STOP=1",
      "-Atc",
      sql,
    ],
    { env: database.composeEnv, timeoutMs: 30_000, display: false }
  );
  if (result.code !== 0)
    throw new HarnessError(
      "Disposable database operation failed: " + sanitizeText(result.stderr)
    );
  return result.stdout.trim();
}

export async function resetDatabase(database) {
  const marker = await databaseSql(
    database,
    "qintopia_test",
    "SELECT run_id FROM public.harness_run"
  );
  if (marker !== database.runId)
    throw new HarnessError("Database run marker mismatch; refusing reset");
  await databaseSql(
    database,
    "qintopia_test",
    `
    DO $reset$ DECLARE item record; BEGIN
      FOR item IN SELECT nspname FROM pg_namespace WHERE nspname LIKE 'qintopia_%' LOOP
        EXECUTE format('DROP SCHEMA %I CASCADE', item.nspname);
      END LOOP;
    END $reset$;
    DROP TABLE IF EXISTS public._sqlx_migrations;
    DROP TABLE public.harness_run;
  `
  );
  await migrateDatabase(database, { runId: database.runId, runDir: database.runDir });
}

export async function stopDatabase(database) {
  if (!database) return { skipped: true };
  const inspect = await runProcess(["docker", "volume", "inspect", database.volume], {
    env: database.composeEnv,
    timeoutMs: 15_000,
    display: false,
  });
  let owned = false;
  if (inspect.code === 0) {
    try {
      const records = JSON.parse(inspect.stdout);
      const labels = records[0]?.Labels ?? {};
      owned =
        labels["com.qintopia.local-testing.owner"] === "local-business-testing" &&
        labels["com.qintopia.local-testing.run-id"] ===
          database.composeEnv.QINTOPIA_TEST_RUN_ID &&
        database.volume.startsWith("qintopia-local-testing-");
    } catch {
      owned = false;
    }
  }
  if (!owned) {
    return {
      owned: false,
      error:
        "owned volume marker was not verified; resources retained: " + database.project,
    };
  }
  const down = await runProcess(
    [...database.base, "down", "--volumes", "--remove-orphans"],
    {
      env: database.composeEnv,
      timeoutMs: 60_000,
      display: false,
    }
  );
  const remains =
    spawnSync("docker", ["volume", "inspect", database.volume], {
      cwd: repoRoot,
      env: database.composeEnv,
      stdio: "ignore",
      timeout: 15000,
    }).status === 0;
  return {
    owned: true,
    downCode: down.code,
    volumeRemoved: !remains,
    error:
      down.code !== 0 || down.timedOut || down.launchError
        ? "Compose teardown did not complete successfully"
        : undefined,
  };
}

export function commandForScenario(feature, scenario, runContext, junitFile) {
  const target = scenario.target;
  const baseEnv = {
    QINTOPIA_TEST_MODE: "1",
    QINTOPIA_TEST_RUN_ID: runContext.runId,
    QINTOPIA_TEST_RUN_DIR: runContext.runDir,
    QINTOPIA_TEST_SCENARIO_ID: scenario.id,
    QINTOPIA_TEST_SIDECAR_BIN: path.join(
      repoRoot,
      "runtime/sidecar/target/debug/qintopia-message-sidecar"
    ),
    QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE: "1",
  };
  const env = pythonEnvironment({ ...baseEnv });
  if (target.pythonpath?.length)
    env.PYTHONPATH = target.pythonpath
      .map((entry) => assertRelativePath(entry, "pythonpath"))
      .join(path.delimiter);
  if (runContext.databaseUrl) {
    env.QINTOPIA_SIDECAR_DATABASE_URL = runContext.databaseUrl;
    env.QINTOPIA_SIDECAR_MIGRATIONS_DIR = path.join(
      repoRoot,
      "runtime/postgres/migrations"
    );
  }
  let argv;
  if (scenario.executor === "pytest") {
    argv = [
      venvPython,
      "-m",
      "pytest",
      "-p",
      "allure_pytest",
      target.path + (target.nodeid ? "::" + target.nodeid : ""),
      "--alluredir",
      runContext.allureResults,
      "--junitxml",
      junitFile,
      ...(target.args ?? []),
    ];
  } else if (scenario.executor === "unittest") {
    const absolute = assertRelativePath(
      target.path,
      `scenario ${scenario.id}.target.path`
    );
    if (fs.statSync(absolute).isDirectory()) {
      argv = [
        venvPython,
        "-m",
        "unittest",
        "discover",
        "-s",
        absolute,
        "-p",
        "test_*.py",
        ...(target.args ?? []),
      ];
    } else {
      argv = [venvPython, "-m", "unittest", absolute, ...(target.args ?? [])];
    }
  } else if (scenario.executor === "cargo") {
    argv = [
      "cargo",
      "test",
      "--locked",
      "--manifest-path",
      target.manifest,
      "--features",
      (target.features ?? []).join(","),
      target.filter,
      ...(target.args ?? []),
    ];
  } else if (scenario.executor === "node") {
    argv = [
      "node",
      "--test",
      "--test-reporter=tap",
      target.path,
      ...(target.args ?? []),
    ];
  } else {
    throw new HarnessError(`unsupported executor: ${scenario.executor}`);
  }
  return {
    argv,
    env,
    cwd: target.cwd ? assertRelativePath(target.cwd, "target.cwd") : repoRoot,
  };
}

export function createAllureResult({
  feature,
  scenario,
  result,
  counts,
  runContext,
  junitFile,
}) {
  const uuid = crypto.randomUUID();
  const start = Date.now() - result.durationMs;
  const hasRequiredSkip = scenario.required && counts.skipped > 0;
  const status =
    result.timedOut ||
    result.launchError ||
    result.outputTooLarge ||
    result.code === null ||
    counts.errors > 0 ||
    counts.total === 0
      ? "broken"
      : result.code !== 0
        ? counts.executed > 0 && !result.timedOut
          ? "failed"
          : "broken"
        : hasRequiredSkip || counts.executed === 0
          ? "skipped"
          : "passed";
  const source = `${uuid}-command.txt`;
  const sanitized = sanitizeText(`${result.stdout}\n${result.stderr}`);
  fs.writeFileSync(path.join(runContext.allureResults, source), sanitized, "utf8");
  const payload = {
    uuid,
    historyId: crypto.createHash("sha256").update(scenario.id).digest("hex"),
    name: scenario.name,
    description: `真实：${scenario.boundary.real.join("、")}\n模拟：${scenario.boundary.simulated.join("、")}`,
    fullName: `${feature.id}/${scenario.id}`,
    status,
    stage: "finished",
    start,
    stop: start + result.durationMs,
    statusDetails: {
      message: result.timedOut
        ? "测试进程超时"
        : counts.total === 0
          ? "没有观察到实际测试用例"
          : hasRequiredSkip
            ? `必需场景存在 ${counts.skipped} 个跳过用例`
            : result.launchError
              ? result.launchError
              : status === "failed"
                ? `原生测试失败：${counts.failed} 个失败；见命令输出附件`
                : status === "broken"
                  ? "测试执行异常；见命令输出附件"
                  : undefined,
      trace: result.stderr ? sanitizeText(result.stderr).slice(-20_000) : undefined,
    },
    labels: [
      { name: "feature", value: feature.name },
      { name: "suite", value: feature.id },
      { name: "framework", value: scenario.executor },
      { name: "scenario_id", value: scenario.id },
    ],
    links: [],
    steps: [
      {
        name: `执行 ${scenario.executor} 测试目标（${counts.executed}/${counts.total}）`,
        status,
        stage: "finished",
        start,
        stop: start + result.durationMs,
        statusDetails: {},
      },
    ],
    attachments: [{ name: "经过筛选的命令输出", source, type: "text/plain" }],
    parameters: [
      { name: "real_boundary", value: scenario.boundary.real.join("; ") },
      { name: "simulated_boundary", value: scenario.boundary.simulated.join("; ") },
    ],
  };
  writeJson(path.join(runContext.allureResults, `${uuid}-result.json`), payload);
  return { status, uuid, junitFile };
}

export function writeSummary(runContext, summary) {
  writeJson(path.join(runContext.runDir, "summary.json"), {
    ...summary,
    run_id: runContext.runId,
    run_dir: runContext.runDir,
    generated_at: new Date().toISOString(),
  });
}

export function latestRunId() {
  if (!fs.existsSync(path.join(testingRoot, "runs"))) return null;
  const candidates = fs
    .readdirSync(path.join(testingRoot, "runs"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => ({
      id: entry.name,
      mtime: fs.statSync(path.join(testingRoot, "runs", entry.name)).mtimeMs,
    }))
    .filter(({ id }) =>
      fs.existsSync(path.join(testingRoot, "runs", id, "summary.json"))
    )
    .sort((a, b) => b.mtime - a.mtime);
  return candidates[0]?.id ?? null;
}

export function runDirectory(runId) {
  if (!/^[0-9a-z-]+$/i.test(runId))
    throw new HarnessError("run id contains unsafe characters");
  const directory = path.join(testingRoot, "runs", runId);
  if (!fs.existsSync(directory)) throw new HarnessError(`run does not exist: ${runId}`);
  return directory;
}

export async function generateAllureReport(runDir) {
  const resultsDir = path.join(runDir, "allure-results");
  const reportDir = path.join(runDir, "allure-report");
  if (!fs.existsSync(resultsDir))
    throw new HarnessError("Allure results directory is missing");
  const result = await runProcess(
    ["pnpm", "exec", "allure", "generate", resultsDir, "--clean", "-o", reportDir],
    {
      env: safeEnvironment(),
      timeoutMs: 180_000,
    }
  );
  if (result.code !== 0) throw new HarnessError("Allure report generation failed");
  return reportDir;
}

export async function openAllureReport(runDir) {
  const reportDir = path.join(runDir, "allure-report");
  if (!fs.existsSync(reportDir)) await generateAllureReport(runDir);
  const result = await runProcess(
    ["pnpm", "exec", "allure", "open", reportDir, "--host", "127.0.0.1"],
    {
      env: safeEnvironment(),
      timeoutMs: 0,
    }
  );
  if (result.code !== 0 && !result.interrupted)
    throw new HarnessError("Allure report server failed");
}

export function doctorChecks() {
  const checks = [];
  const commandCheck = (name, command) =>
    checks.push({ name, ok: commandExists(command), detail: command });
  commandCheck("Node.js", "node");
  commandCheck("pnpm", "pnpm");
  commandCheck("Python 3", "python3");
  commandCheck("Rust cargo", "cargo");
  commandCheck("Docker", "docker");
  const compose = spawnSync("docker", ["compose", "version"], {
    cwd: repoRoot,
    env: safeEnvironment(),
    stdio: "ignore",
    timeout: 15000,
  });
  const daemon = spawnSync("docker", ["info", "--format", "{{.ServerVersion}}"], {
    env: safeEnvironment(),
    stdio: "ignore",
    timeout: 15000,
  });
  checks.push({
    name: "Docker daemon",
    ok: daemon.status === 0,
    detail: "docker info",
  });
  checks.push({
    name: "Docker Compose",
    ok: compose.status === 0,
    detail: "docker compose version",
  });
  const java = spawnSync("java", ["-version"], {
    cwd: repoRoot,
    env: safeEnvironment(),
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout: 15000,
  });
  const javaText = `${java.stdout ?? ""}\n${java.stderr ?? ""}`;
  const javaMajor = Number(javaText.match(/version\s+"(\d+)/)?.[1] ?? 0);
  checks.push({
    name: "Java 17+",
    ok: java.status === 0 && javaMajor >= 17,
    detail: `java major ${javaMajor || "unknown"}`,
  });
  const allure = spawnSync("pnpm", ["exec", "allure", "--version"], {
    cwd: repoRoot,
    env: safeEnvironment(),
    stdio: "ignore",
    timeout: 30000,
  });
  checks.push({
    name: "Allure CLI",
    ok: allure.status === 0,
    detail: "pnpm exec allure --version",
  });
  checks.push({
    name: "catalog schema",
    ok: fs.existsSync(catalogPath) && fs.existsSync(catalogSchemaPath),
    detail: catalogPath,
  });
  checks.push({
    name: "compose file",
    ok: fs.existsSync(composePath),
    detail: composePath,
  });
  checks.push({
    name: "test Python venv",
    ok: fs.existsSync(venvPython),
    detail: venvPython,
  });
  return checks;
}

export function reportChecks(checks) {
  for (const check of checks)
    process.stdout.write(
      `${check.ok ? "OK" : "MISSING"} ${check.name}: ${check.detail}\n`
    );
  return checks.every((check) => check.ok);
}
