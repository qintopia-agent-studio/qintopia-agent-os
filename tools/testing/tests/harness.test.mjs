import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs";
import path from "node:path";
import {
  validateCatalog,
  selectScenarios,
  runCount,
  safeEnvironment,
  runProcess,
  databaseSpec,
  repoRoot,
  runDirectory,
  createAllureResult,
  commandForScenario,
} from "../lib.mjs";

test("catalog resolves full, feature, alias and parameterized scenario without duplicate execution", () => {
  const { catalog } = validateCatalog();
  const all = selectScenarios(catalog);
  assert.equal(new Set(all.map((x) => x.scenario.id)).size, all.length);
  assert.equal(selectScenarios(catalog, { feature: "erhua-morning-brief" }).length, 5);
  assert.equal(selectScenarios(catalog, { feature: "二花早报" }).length, 5);
  const one = selectScenarios(catalog, {
    scenario: "erhua-morning-brief/duplicate-send",
  });
  assert.equal(one.length, 1);
  assert.equal(one[0].scenario.target.nodeid, "test_text_delivery[duplicate-send]");
  assert.throws(() => selectScenarios(catalog, { feature: "unknown" }));
  assert.throws(() =>
    selectScenarios(catalog, { feature: "qiwe", scenario: "unknown" })
  );
  const bad = structuredClone(catalog);
  bad.businesses[0].scenarios.push(bad.businesses[0].scenarios[0]);
  assert.throws(() => validateCatalog(bad), /duplicate|collides/);
  const missing = structuredClone(catalog);
  missing.businesses[0].scenarios[0].target.path = "not-present.py";
  assert.throws(() => validateCatalog(missing), /does not exist/);
  const broad = structuredClone(catalog);
  const cargoCase = broad.businesses
    .flatMap((x) => x.scenarios)
    .find((x) => x.executor === "cargo");
  cargoCase.target.args = ["--", "--ignored"];
  assert.throws(() => validateCatalog(broad), /exact filter/);
});

test("native test counts cannot turn zero matches or skips into execution", () => {
  const result = {
    stdout: "test result: ok. 0 passed; 0 failed; 0 ignored; 800 filtered out;",
    stderr: "",
  };
  assert.equal(runCount(result, "cargo").executed, 0);
  assert.equal(
    runCount({ stdout: "", stderr: "Ran 3 tests in 0.3s\nOK (skipped=1)" }, "unittest")
      .executed,
    2
  );
  assert.equal(
    runCount({ stdout: "# tests 2\n# skipped 1\n# fail 0", stderr: "" }, "node_test")
      .executed,
    1
  );
  assert.equal(runCount({ stdout: "successful", stderr: "" }, "cargo"), null);
});

test("test subprocesses omit ambient application environment", () => {
  process.env.QINTOPIA_SIDECAR_DATABASE_URL = "must-not-inherit";
  process.env.QIWE_TOKEN = "must-not-inherit";
  const env = safeEnvironment();
  assert.equal(env.QINTOPIA_SIDECAR_DATABASE_URL, undefined);
  assert.equal(env.QIWE_TOKEN, undefined);
  assert.equal(env.PYTEST_DISABLE_PLUGIN_AUTOLOAD, "1");
  delete process.env.QINTOPIA_SIDECAR_DATABASE_URL;
  delete process.env.QIWE_TOKEN;
});

test("process launch failure and resistant process timeout terminate", async () => {
  const missing = await runProcess(["qintopia-nonexistent-executable"], {
    display: false,
  });
  assert.ok(missing.launchError);
  const slow = await runProcess(
    [process.execPath, "-e", "process.on('SIGTERM',()=>{}); setInterval(()=>{},1000)"],
    { display: false, timeoutMs: 100 }
  );
  assert.equal(slow.timedOut, true);
  assert.ok(slow.durationMs < 5000);
  const failure = await runProcess([process.execPath, "-e", "process.exit(7)"], {
    display: false,
  });
  assert.equal(failure.code, 7);
});

test("database resources and report selection stay within run ownership", () => {
  const a = databaseSpec({
    runId: "20260913-a",
    runDir: path.join(repoRoot, ".local-testing/runs/a"),
  });
  const b = databaseSpec({
    runId: "20260913-b",
    runDir: path.join(repoRoot, ".local-testing/runs/b"),
  });
  assert.notEqual(a.volume, b.volume);
  assert.notEqual(a.project, b.project);
  assert.equal(a.composeEnv.QINTOPIA_TEST_RUN_ID, "20260913-a");
  assert.throws(() => runDirectory("../../"));
  assert.ok(
    fs
      .readFileSync(path.join(repoRoot, "tools/testing/compose.yml"), "utf8")
      .includes("127.0.0.1::5432")
  );
});

test("reports fail closed for empty, broken and required-skipped groups and preserve failure evidence", () => {
  const root = path.join(repoRoot, ".local-testing");
  fs.mkdirSync(root, { recursive: true });
  const directory = fs.mkdtempSync(path.join(root, "harness-report-"));
  const feature = { id: "sample", name: "示例" };
  const scenario = {
    id: "sample/case",
    name: "场景",
    required: true,
    executor: "node",
    boundary: { real: ["module"], simulated: ["channel"] },
  };
  const result = { code: 0, durationMs: 10, stdout: "observed", stderr: "" };
  const counts = { total: 1, executed: 1, skipped: 0, failed: 0 };
  try {
    for (const [patch, tally, expected] of [
      [{}, {}, "passed"],
      [{ code: 1, stderr: "expected 1 actual 2" }, { failed: 1 }, "failed"],
      [{}, { total: 0, executed: 0 }, "broken"],
      [{ timedOut: true }, {}, "broken"],
      [{ code: null }, {}, "broken"],
      [{}, { errors: 1 }, "broken"],
      [{}, { executed: 0, skipped: 1 }, "skipped"],
    ]) {
      const output = createAllureResult({
        feature,
        scenario,
        result: { ...result, ...patch },
        counts: { ...counts, ...tally },
        runContext: { allureResults: directory },
      });
      assert.equal(output.status, expected);
      const report = JSON.parse(
        fs.readFileSync(path.join(directory, output.uuid + "-result.json"))
      );
      assert.equal(report.status, expected);
      assert.ok(fs.existsSync(path.join(directory, report.attachments[0].source)));
    }
    assert.equal(
      fs.readdirSync(directory).filter((x) => x.endsWith("-result.json")).length,
      7
    );
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test("legacy QiWe tests get only their declared import directory", () => {
  const { catalog } = validateCatalog();
  const { feature, scenario } = selectScenarios(catalog, { feature: "qiwe" })[0];
  const command = commandForScenario(feature, scenario, {
    runId: "test",
    runDir: "/tmp/test",
  });
  assert.equal(command.env.PYTHONPATH, path.join(repoRoot, "skills/qiwe"));
  assert.ok(command.argv.includes(path.join(repoRoot, scenario.target.path)));
  assert.equal(command.cwd, path.join(repoRoot, "skills/qiwe"));
});
