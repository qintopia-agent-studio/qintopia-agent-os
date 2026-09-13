#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import {
  HarnessError,
  repoRoot,
  testingRoot,
  venvPython,
  venvPath,
  requirementsPath,
  validateCatalog,
  selectScenarios,
  ensureDir,
  makeRunId,
  writeJson,
  safeEnvironment,
  pythonEnvironment,
  runProcess,
  runCount,
  commandForScenario,
  createAllureResult,
  databaseSpec,
  startDatabase,
  migrateDatabase,
  resetDatabase,
  stopDatabase,
  doctorChecks,
  reportChecks,
  wasInterrupted,
  generateAllureReport,
  openAllureReport,
  sanitizeText,
  latestRunId,
  runDirectory,
  writeSummary,
} from "./lib.mjs";

function options(args) {
  const result = {};
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--") continue;
    if (
      !["--feature", "--scenario", "--run"].includes(args[i]) ||
      !args[i + 1] ||
      args[i + 1].startsWith("--")
    )
      throw new HarnessError("Use --feature <id>, --scenario <id> or --run <id>");
    const name = args[i].slice(2);
    if (result[name]) throw new HarnessError("Repeated option: " + name);
    result[name] = args[++i];
  }
  return result;
}

async function checked(argv, options = {}) {
  const result = await runProcess(argv, options);
  if (result.code !== 0 || result.timedOut || result.interrupted)
    throw new HarnessError("Command failed: " + argv[0], result.interrupted ? 130 : 2);
  return result;
}

async function setup() {
  ensureDir(testingRoot);
  if (!fs.existsSync(venvPython)) await checked(["python3", "-m", "venv", venvPath]);
  await checked(
    [
      venvPython,
      "-m",
      "pip",
      "install",
      "--disable-pip-version-check",
      "-r",
      requirementsPath,
    ],
    { timeoutMs: 300000 }
  );
  await checked([venvPython, "-m", "pip", "check"]);
  console.log("Testing environment ready. Run pnpm test:doctor.");
}

async function business(selected) {
  const runId = makeRunId();
  const runDir = ensureDir(path.join(testingRoot, "runs", runId));
  const context = {
    runId,
    runDir,
    allureResults: ensureDir(path.join(runDir, "allure-results")),
  };
  const summary = {
    status: "running",
    selected: selected.map((x) => x.scenario.id),
    results: [],
    errors: [],
    started_at: new Date().toISOString(),
  };
  let database = null;
  let exitCode = 0;
  fs.writeFileSync(path.join(runDir, ".env"), "# Empty local test environment\n");
  writeJson(path.join(runDir, "context.json"), { run_id: runId, run_dir: runDir });
  writeSummary(context, summary);
  try {
    if (!fs.existsSync(venvPython)) throw new HarnessError("Run pnpm test:setup first");
    await checked([venvPython, "-c", "import pytest,allure"], { display: false });
    const needsPostgres = selected.some((x) =>
      x.scenario.requires.includes("postgres")
    );
    if (needsPostgres) {
      await checked(
        ["cargo", "build", "--locked", "--manifest-path", "runtime/sidecar/Cargo.toml"],
        { timeoutMs: 900000 }
      );
      database = databaseSpec(context);
      await startDatabase(database);
      await migrateDatabase(database, context);
      context.databaseUrl = database.databaseUrl;
    }
    for (const { feature, scenario } of selected) {
      if (wasInterrupted()) throw new HarnessError("Interrupted", 130);
      console.log(
        `\n[${summary.results.length + 1}/${selected.length}] ${feature.name} · ${scenario.name}`
      );
      if (
        database &&
        summary.results.some((r) => r.requires_postgres) &&
        scenario.requires.includes("postgres")
      )
        await resetDatabase(database);
      const junitFile = path.join(runDir, scenario.id.replaceAll("/", "-") + ".xml");
      const command = commandForScenario(feature, scenario, context, junitFile);
      const result = await runProcess(command.argv, {
        env: command.env,
        cwd: command.cwd,
        timeoutMs: scenario.timeout_seconds * 1000,
      });
      const counts = runCount(result, scenario.target.count_strategy, junitFile) ?? {
        total: 0,
        executed: 0,
        skipped: 0,
        failed: 0,
      };
      let status;
      if (
        scenario.executor === "pytest" &&
        counts.total > 0 &&
        !result.timedOut &&
        !result.outputTooLarge &&
        result.code !== null &&
        !result.launchError
      ) {
        status =
          counts.errors > 0 || result.code > 1
            ? "broken"
            : counts.failed > 0 || result.code === 1
              ? "failed"
              : counts.skipped > 0
                ? "skipped"
                : "passed";
      } else {
        status = createAllureResult({
          feature,
          scenario,
          result,
          counts,
          runContext: context,
          junitFile,
        }).status;
      }
      if (result.interrupted) exitCode = 130;
      else if (status === "broken") exitCode = Math.max(exitCode, 2);
      else if (status === "failed") exitCode = Math.max(exitCode, 1);
      else if (status === "skipped" && scenario.required)
        exitCode = Math.max(exitCode, 2);
      summary.results.push({
        scenario_id: scenario.id,
        requires_postgres: scenario.requires.includes("postgres"),
        name: scenario.name,
        status,
        counts,
        duration_ms: result.durationMs,
        boundary: scenario.boundary,
      });
      console.log(
        `${status.toUpperCase()} ${scenario.name} (${counts.executed} executed, ${counts.skipped} skipped)`
      );
      writeSummary(context, summary);
    }
  } catch (error) {
    exitCode = error.code === 130 ? 130 : 2;
    summary.errors.push(error.message);
    console.error(error.message);
  } finally {
    for (const { feature, scenario } of selected.filter(
      (x) => !summary.results.some((r) => r.scenario_id === x.scenario.id)
    )) {
      createAllureResult({
        feature,
        scenario,
        counts: { total: 0, executed: 0, skipped: 0, failed: 0 },
        result: {
          durationMs: 0,
          code: null,
          launchError: summary.errors.join("; ") || "Not executed",
          stdout: "",
          stderr: summary.errors.join("; ") || "Not executed",
        },
        runContext: context,
      });
      summary.results.push({
        scenario_id: scenario.id,
        name: scenario.name,
        status: "broken",
        reason: "Not executed",
        counts: { total: 0, executed: 0, skipped: 0, failed: 0 },
      });
    }
    try {
      if (database) {
        const logs = await runProcess(
          [...database.base, "logs", "--no-color", "postgres"],
          { env: database.composeEnv, display: false, timeoutMs: 15000 }
        );
        fs.writeFileSync(
          path.join(runDir, "postgres.log"),
          sanitizeText(logs.stdout + logs.stderr)
        );
      }
      summary.cleanup = await stopDatabase(database);
      if (
        summary.cleanup.error ||
        (summary.cleanup.owned && summary.cleanup.downCode !== 0) ||
        summary.cleanup.volumeRemoved === false
      ) {
        summary.errors.push(summary.cleanup.error || "Database cleanup incomplete");
        if (exitCode !== 130) exitCode = 2;
      }
    } catch (error) {
      summary.errors.push("Cleanup: " + error.message);
      if (exitCode !== 130) exitCode = 2;
    }
    try {
      summary.report = await generateAllureReport(runDir);
    } catch (error) {
      summary.errors.push(error.message);
      if (exitCode !== 130) exitCode = 2;
    }
    if (wasInterrupted()) exitCode = 130;
    summary.status =
      exitCode === 0
        ? "passed"
        : exitCode === 1
          ? "failed"
          : exitCode === 130
            ? "interrupted"
            : "broken";
    summary.exit_code = exitCode;
    summary.finished_at = new Date().toISOString();
    writeSummary(context, summary);
    console.log(
      `\n${summary.status.toUpperCase()} · run ${runId}\nReport: pnpm test:report -- --run ${runId}`
    );
  }
  return exitCode;
}

try {
  const [command, ...args] = process.argv.slice(2);
  const opts = options(args);
  if (command === "setup") await setup();
  else if (command === "doctor")
    process.exitCode = reportChecks(doctorChecks()) ? 0 : 2;
  else if (command === "harness") {
    const result = await runProcess([
      "node",
      "--test",
      "tools/testing/tests/harness.test.mjs",
    ]);
    process.exitCode = result.code === 0 ? 0 : 2;
  } else if (command === "list" || command === "business") {
    if (opts.run) throw new HarnessError("--run is only supported by report");
    const { catalog } = validateCatalog();
    const selected = selectScenarios(catalog, opts);
    if (command === "list") {
      for (const { feature, scenario } of selected)
        console.log(
          `${feature.id} · ${scenario.id} · ${scenario.name}\n  real: ${scenario.boundary.real.join(" / ")}\n  simulated: ${scenario.boundary.simulated.join(" / ")}`
        );
      console.log("\nNot yet covered:");
      for (const gap of catalog.gaps) console.log(`- ${gap.name}: ${gap.reason}`);
    } else process.exitCode = await business(selected);
  } else if (command === "report") {
    if (opts.feature || opts.scenario)
      throw new HarnessError("report accepts only --run");
    const id = opts.run || latestRunId();
    if (!id) throw new HarnessError("No test report exists yet");
    await openAllureReport(runDirectory(id));
  } else throw new HarnessError("Use setup, doctor, list, business, report or harness");
} catch (error) {
  console.error(error.message);
  process.exitCode = error.code === 130 ? 130 : 2;
}
