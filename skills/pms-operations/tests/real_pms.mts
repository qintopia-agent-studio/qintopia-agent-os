// Run from the read-only PMS checkout through its database-suite lock wrapper.
// The owning task must provide a disposable loopback PostgreSQL instance.
import { pathToFileURL, fileURLToPath } from "node:url";
import { resolve, dirname } from "node:path";
import { spawn } from "node:child_process";
const url = new URL(process.env.TEST_DATABASE_URL ?? "");
if (
  process.env.ANAN_REAL_PMS_TEST_ENABLE !== "1" ||
  url.hostname !== "127.0.0.1" ||
  url.pathname !== "/qintopia_test" ||
  !url.port ||
  url.search ||
  url.hash
) {
  throw new Error("explicit disposable PMS test instance required");
}
const fromPms = (name: string) =>
  import(pathToFileURL(resolve(process.cwd(), name)).href);
const { resetDatabase } = await fromPms("tests/helpers/database.ts");
const { buildServer } = await fromPms("apps/api/src/server.ts");
const { demo } = await fromPms("packages/db/src/seed.ts");
const db = await resetDatabase(url.toString());
const app = await buildServer(db);
try {
  await app.listen({ host: "127.0.0.1", port: 0 });
  const address = app.server.address();
  if (!address || typeof address === "string")
    throw new Error("missing loopback listener");
  const exit = await new Promise<number | null>((done, reject) => {
    const full = process.env.ANAN_FULL_CHAIN === "1";
    const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    const child = spawn(
      full ? "cargo" : "python3",
      full
        ? [
            "test",
            "--locked",
            "--manifest-path",
            resolve(root, "runtime/sidecar/Cargo.toml"),
            "--features",
            "postgres-integration-tests",
            "pms_local_journey_tests::real_broker_pms_journey",
            "--",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
          ]
        : [resolve(dirname(fileURLToPath(import.meta.url)), "real_pms_journey.py")],
      {
        stdio: "inherit",
        env: {
          ...process.env,
          ANAN_PMS_TEST_BASE_URL: `http://127.0.0.1:${address.port}`,
          ANAN_PMS_TEST_DEMO: JSON.stringify(demo),
        },
      }
    );
    child.on("error", reject);
    child.on("exit", done);
  });
  if (exit !== 0) throw new Error(`real PMS journey failed: ${exit}`);
} finally {
  await app.close();
  await db.destroy();
}
