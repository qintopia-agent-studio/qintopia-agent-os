#!/usr/bin/env node

import path from "node:path";
import process from "node:process";
import {
  resolveApprovedTarget,
  resolveContainedArtifactDir,
} from "./sidecar-artifact-build-boundary.mjs";

const originalTarget = process.env.QINTOPIA_ARTIFACT_TARGET;

try {
  process.env.QINTOPIA_ARTIFACT_TARGET = "linux-x86_64-gnu/../../escape";
  assertThrows(() => resolveApprovedTarget(), "target traversal must be rejected");

  process.env.QINTOPIA_ARTIFACT_TARGET = "linux-x86_64-gnu";
  if (
    resolveApprovedTarget({
      platform: "linux",
      arch: "x64",
      glibcVersionRuntime: "2.35",
    }) !== "linux-x86_64-gnu"
  ) {
    throw new Error("approved target was not returned");
  }
  assertThrows(
    () =>
      resolveApprovedTarget({
        platform: "linux",
        arch: "x64",
        glibcVersionRuntime: null,
      }),
    "linux-x64 non-GNU hosts must not build linux-x86_64-gnu artifacts"
  );
  assertThrows(
    () =>
      resolveApprovedTarget({
        platform: "darwin",
        arch: "x64",
        glibcVersionRuntime: undefined,
      }),
    "non-linux-x64 hosts must not build linux-x86_64-gnu artifacts"
  );

  const root = path.join(process.cwd(), "dist", ".test-sidecar-artifact-root");
  const inside = resolveContainedArtifactDir(
    root,
    "qintopia-message-sidecar-linux-x86_64-gnu"
  );
  if (!inside.startsWith(`${path.resolve(root)}${path.sep}`)) {
    throw new Error("contained artifact dir escaped output root");
  }

  for (const artifactName of [
    "../escape",
    "qintopia/escape",
    "qintopia\\escape",
    ".hidden-artifact",
    "..-escape",
  ]) {
    assertThrows(
      () => resolveContainedArtifactDir(root, artifactName),
      `artifact name ${artifactName} must be rejected`
    );
  }
} finally {
  if (originalTarget === undefined) {
    delete process.env.QINTOPIA_ARTIFACT_TARGET;
  } else {
    process.env.QINTOPIA_ARTIFACT_TARGET = originalTarget;
  }
}

console.log("Sidecar artifact build boundary test passed.");

function assertThrows(callback, message) {
  try {
    callback();
  } catch {
    return;
  }
  throw new Error(message);
}
