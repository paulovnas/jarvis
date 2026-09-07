import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { RELEASE_REPOSITORY, replaceCargoVersion } from "./release-plan";

export const root = fileURLToPath(new URL("../", import.meta.url));
export const versionFiles = ["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"];
export const releaseTarget = "aarch64-apple-darwin";
export const releaseWorkflow = "release-macos.yml";
export const releaseEnvironment = "macos-release";
export const requiredSecrets = ["TAURI_SIGNING_PRIVATE_KEY", "APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_SIGNING_IDENTITY"];
export const read = (file: string) => readFileSync(path.join(root, file), "utf8");

function commandEnvironment(env: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  // Git and gh treat "cat" as a no-pager sentinel on every supported platform.
  // Keep automation independent of the user's interactive terminal preferences.
  const childEnv: NodeJS.ProcessEnv = {
    ...env, GH_PAGER: "cat", GIT_PAGER: "cat", PAGER: "cat",
    GH_PROMPT_DISABLED: "1", GIT_TERMINAL_PROMPT: "0", GH_NO_UPDATE_NOTIFIER: "1",
  };
  delete childEnv.GH_FORCE_TTY;
  return childEnv;
}

export function command(program: string, args: string[], capture = false, env = process.env): string {
  const result = spawnSync(program, args, { cwd: root, env: commandEnvironment(env), encoding: "utf8", stdio: capture ? ["ignore", "pipe", "pipe"] : ["ignore", "inherit", "inherit"] });
  if (result.error || result.status !== 0) throw new Error(`Falhou: ${program} ${args.join(" ")}${capture && result.stderr ? `\n${result.stderr.trim()}` : ""}`);
  return result.stdout?.trim() ?? "";
}

export function configuration() {
  const pkg = JSON.parse(read("package.json")) as { version: string };
  const config = JSON.parse(read("src-tauri/tauri.conf.json")) as { version: string; productName: string; plugins: { updater: { pubkey: string } } };
  if (config.version !== pkg.version || versionFiles.slice(2).some(file => replaceCargoVersion(read(file), pkg.version, file.endsWith(".lock")) !== read(file))) {
    throw new Error("As versões em package.json, Tauri e Cargo devem coincidir.");
  }
  return { pkg, config };
}

export function optionalRelease(tag: string): { isDraft: boolean; body: string } | null {
  const result = spawnSync("gh", ["release", "view", tag, "--repo", RELEASE_REPOSITORY, "--json", "isDraft,body"], { cwd: root, env: commandEnvironment(process.env), encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  if (result.status === 0) return JSON.parse(result.stdout);
  if (/release not found|HTTP 404/i.test(result.stderr)) return null;
  throw new Error("Não foi possível consultar o release no GitHub. Confira gh auth status.");
}
