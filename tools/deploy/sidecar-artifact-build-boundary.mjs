import path from "node:path";
import process from "node:process";

const approvedTarget = "linux-x86_64-gnu";
const artifactNamePattern = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

export const resolveApprovedTarget = ({
  platform = process.platform,
  arch = process.arch,
  glibcVersionRuntime = process.report?.getReport?.()?.header?.glibcVersionRuntime,
} = {}) => {
  const target = process.env.QINTOPIA_ARTIFACT_TARGET ?? approvedTarget;
  if (target !== approvedTarget) {
    throw new Error(`QINTOPIA_ARTIFACT_TARGET must be ${approvedTarget}`);
  }
  if (platform !== "linux" || arch !== "x64") {
    throw new Error(`${approvedTarget} artifacts must be built on linux x64 runners`);
  }
  if (!glibcVersionRuntime) {
    throw new Error(
      `${approvedTarget} artifacts must be built on linux x64 GNU runners`
    );
  }
  return target;
};

export const resolveContainedArtifactDir = (outputRoot, artifactName) => {
  if (
    !artifactNamePattern.test(artifactName) ||
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
