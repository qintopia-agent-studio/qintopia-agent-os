#!/usr/bin/env node

import os from "node:os";
import process from "node:process";
import { commandExists, run } from "./run-command.mjs";

if (commandExists("gh")) {
  console.log("GitHub CLI is already installed.");
  try {
    run("gh", ["auth", "status"], { stdio: ["ignore", "pipe", "pipe"] });
    console.log("GitHub CLI is authenticated.");
  } catch {
    console.log("Run gh auth login before creating PRs.");
  }
  process.exit(0);
}

const platform = process.platform;
const autoInstall = process.argv.includes("--install");

const printManualInstall = () => {
  console.log("GitHub CLI is required for repository-owned PR creation.");
  console.log("Install one of:");
  console.log("- macOS Homebrew: brew install gh");
  console.log("- Windows WinGet: winget install --id GitHub.cli");
  console.log(
    "- Debian/Ubuntu: follow https://github.com/cli/cli/blob/trunk/docs/install_linux.md"
  );
  console.log("Then run: gh auth login");
};

if (!autoInstall) {
  printManualInstall();
  console.log(
    "To allow this script to install when a supported package manager exists, run:"
  );
  console.log("pnpm pr:bootstrap -- --install");
  process.exit(1);
}

if (platform === "darwin" && commandExists("brew")) {
  run("brew", ["install", "gh"], { stdio: "inherit" });
} else if (platform === "win32" && commandExists("winget")) {
  run("winget", ["install", "--id", "GitHub.cli"], { stdio: "inherit" });
} else if (platform === "linux" && commandExists("apt-get")) {
  console.log(
    "Automatic apt installation may require sudo. Using GitHub CLI official apt repo flow."
  );
  run(
    "bash",
    [
      "-lc",
      'type -p curl >/dev/null || sudo apt-get update && sudo apt-get install -y curl && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg && sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null && sudo apt-get update && sudo apt-get install -y gh',
    ],
    { stdio: "inherit" }
  );
} else {
  printManualInstall();
  console.error(
    `Unsupported automatic install environment: ${platform} ${os.release()}`
  );
  process.exit(1);
}

console.log("GitHub CLI installed. Run gh auth login before creating PRs.");
