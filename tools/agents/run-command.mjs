import { execFileSync } from "node:child_process";

export const run = (command, args, options = {}) => {
  const output = execFileSync(command, args, {
    encoding: "utf8",
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  });
  return typeof output === "string" ? output.trim() : "";
};

export const commandExists = (command) => {
  try {
    run("sh", ["-lc", `command -v ${JSON.stringify(command)}`], {
      stdio: ["ignore", "pipe", "ignore"],
    });
    return true;
  } catch {
    return false;
  }
};
