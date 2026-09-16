import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "deploy-request-clock-"));
try {
  const preload = path.join(temporary, "clock.cjs");
  fs.writeFileSync(
    preload,
    `
const RealDate = Date;
let now = RealDate.parse('2026-09-16T00:00:00.000Z');
global.Date = class extends RealDate {
  constructor(...args) { super(...(args.length ? args : [now++])); }
  static now() { return now++; }
};
`
  );
  for (const minutes of [60, 30]) {
    const output = path.join(temporary, `request-${minutes}.json`);
    execFileSync(
      process.execPath,
      [
        "--require",
        preload,
        "tools/deploy/create-deploy-request.mjs",
        "--commit-sha",
        "a".repeat(40),
        "--release-scope",
        "deploy-bundle,hermes-plugins",
        "--restart-targets",
        "hermes-erhua",
        "--requested-by",
        "clock-fixture",
        "--cos-bucket",
        "fixture-1234567890",
        "--cos-region",
        "ap-shanghai",
        "--ttl-minutes",
        String(minutes),
        "--dry-run",
        "true",
        "--output",
        output,
      ],
      {
        env: {
          PATH: process.env.PATH,
          DEPLOY_REQUEST_SIGNING_KEY: "isolated-clock-fixture-key",
        },
        stdio: "pipe",
      }
    );
    const request = JSON.parse(fs.readFileSync(output, "utf8"));
    assert.equal(
      Date.parse(request.expires_at) - Date.parse(request.created_at),
      minutes * 60_000
    );
    assert.ok(
      Math.abs(
        Date.parse(request.signature.signed_at) - Date.parse(request.created_at)
      ) <=
        5 * 60_000
    );
  }
  console.log("Deploy request advancing-clock tests passed.");
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
