import path from "node:path";
import process from "node:process";

const approvedTarget = "linux-x86_64-gnu";

export const resolveApprovedTarget = () => {
  const target = process.env.QINTOPIA_ARTIFACT_TARGET ?? approvedTarget;
  if (target !== approvedTarget) {
    throw new Error(`QINTOPIA_ARTIFACT_TARGET must be ${approvedTarget}`);
  }
  if (process.platform !== "linux" || process.arch !== "x64") {
    throw new Error(`${approvedTarget} artifacts must be built on linux x64 runners`);
  }
  return target;
};

export const resolveContainedArtifactDir = (outputRoot, artifactName) => {
  if (
    artifactName.includes("/") ||
    artifactName.includes("\\") ||
    artifactName.split("-").includes("..")
  ) {
    throw new Error("artifact name must not contain path traversal components");
  }
  const resolvedRoot = path.resolve(outputRoot);
  const resolvedDir = path.resolve(resolvedRoot, artifactName);
  if (
    resolvedDir === resolvedRoot ||
    !resolvedDir.startsWith(`${resolvedRoot}${path.sep}`)
  ) {
    throw new Error("artifact output directory must stay under dist/sidecar-artifacts");
  }
  return resolvedDir;
};
