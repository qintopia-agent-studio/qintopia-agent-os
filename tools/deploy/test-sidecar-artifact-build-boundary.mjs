#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import {
  assertContainedArtifactDirBoundary,
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

  const fixtureRoot = path.join(
    process.cwd(),
    "dist",
    ".test-sidecar-artifact-boundary"
  );
  fs.rmSync(fixtureRoot, { recursive: true, force: true });
  fs.mkdirSync(fixtureRoot, { recursive: true });

  const root = path.join(fixtureRoot, "safe-root");
  const inside = resolveContainedArtifactDir(
    root,
    "qintopia-message-sidecar-linux-x86_64-gnu"
  );
  if (!inside.startsWith(`${path.resolve(root)}${path.sep}`)) {
    throw new Error("contained artifact dir escaped output root");
  }
  if (!fs.statSync(root).isDirectory()) {
    throw new Error("output root was not safely created");
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

  const outsideRoot = path.join(fixtureRoot, "outside-root");
  const symlinkRoot = path.join(fixtureRoot, "symlink-root");
  fs.mkdirSync(outsideRoot);
  fs.symlinkSync(outsideRoot, symlinkRoot, "dir");
  assertThrows(
    () =>
      resolveContainedArtifactDir(
        symlinkRoot,
        "qintopia-message-sidecar-linux-x86_64-gnu"
      ),
    "symlink output root must be rejected"
  );
  assertThrows(
    () =>
      resolveContainedArtifactDir(
        path.join(symlinkRoot, "sidecar-artifacts"),
        "qintopia-message-sidecar-linux-x86_64-gnu"
      ),
    "symlink output parent must be rejected"
  );

  const fileRoot = path.join(fixtureRoot, "file-root");
  fs.writeFileSync(fileRoot, "not a directory\n");
  assertThrows(
    () =>
      resolveContainedArtifactDir(
        fileRoot,
        "qintopia-message-sidecar-linux-x86_64-gnu"
      ),
    "file output root must be rejected"
  );

  const artifactSymlinkRoot = path.join(fixtureRoot, "artifact-symlink-root");
  const artifactSymlinkTarget = path.join(fixtureRoot, "artifact-symlink-target");
  const artifactSymlinkName = "qintopia-message-sidecar-linux-x86_64-gnu";
  fs.mkdirSync(artifactSymlinkRoot);
  fs.mkdirSync(artifactSymlinkTarget);
  fs.symlinkSync(
    artifactSymlinkTarget,
    path.join(artifactSymlinkRoot, artifactSymlinkName),
    "dir"
  );
  assertThrows(
    () => resolveContainedArtifactDir(artifactSymlinkRoot, artifactSymlinkName),
    "symlink artifact directory must be rejected"
  );

  const revalidatedRoot = path.join(fixtureRoot, "revalidated-root");
  const revalidatedDir = resolveContainedArtifactDir(
    revalidatedRoot,
    "qintopia-message-sidecar-linux-x86_64-gnu"
  );
  fs.rmSync(revalidatedRoot, { recursive: true, force: true });
  fs.symlinkSync(outsideRoot, revalidatedRoot, "dir");
  assertThrows(
    () =>
      assertContainedArtifactDirBoundary(
        revalidatedRoot,
        "qintopia-message-sidecar-linux-x86_64-gnu",
        revalidatedDir
      ),
    "replaced output root symlink must be rejected before writes"
  );
  assertThrows(
    () =>
      assertContainedArtifactDirBoundary(
        root,
        "qintopia-message-sidecar-linux-x86_64-gnu",
        path.join(fixtureRoot, "outside-artifact-dir")
      ),
    "expected artifact directory mismatch must be rejected"
  );
} finally {
  fs.rmSync(path.join(process.cwd(), "dist", ".test-sidecar-artifact-boundary"), {
    recursive: true,
    force: true,
  });
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
