import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  root: "", command: vi.fn<(...args: unknown[]) => string>(),
  optionalRelease: vi.fn<() => { isDraft: boolean } | null>(),
}));
vi.mock("./release-common", () => ({
  get root() { return state.root; },
  versionFiles: ["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"],
  releaseTarget: "aarch64-apple-darwin", releaseWorkflow: "release-macos.yml", releaseEnvironment: "macos-release",
  requiredSecrets: ["KEY"], command: state.command, optionalRelease: state.optionalRelease,
  configuration: () => ({ pkg: JSON.parse(readFileSync(path.join(state.root, "package.json"), "utf8")) as { version: string }, config: { version: "0.8.3-beta.1" } }),
  read: (name: string) => readFileSync(path.join(state.root, name), "utf8"),
}));

const originalArgv = process.argv;
const originalExit = process.exitCode;
beforeEach(() => {
  vi.resetModules();
  state.root = mkdtempSync(path.join(tmpdir(), "jarvis-launcher-test-"));
  mkdirSync(path.join(state.root, "src-tauri"));
  writeFileSync(path.join(state.root, "package.json"), JSON.stringify({ version: "0.8.3-beta.1", scripts: { untouched: "value" } }));
  writeFileSync(path.join(state.root, "src-tauri/Cargo.toml"), '[package]\nname = "jarvis"\nversion = "0.8.3-beta.1"\n');
  writeFileSync(path.join(state.root, "src-tauri/Cargo.lock"), '[[package]]\nname = "jarvis"\nversion = "0.8.3-beta.1"\n');
  state.optionalRelease.mockReset().mockReturnValue(null);
  state.command.mockReset().mockImplementation((program, input) => {
    const args = input as string[];
    const text = args.join(" ");
    if (program === "git" && text === "branch --show-current") return "main";
    if (program === "git" && text.startsWith("rev-parse")) return "abc123";
    if (program === "gh" && text.startsWith("repo view")) return JSON.stringify({ nameWithOwner: "paulovnas/jarvis", isPrivate: false });
    if (program === "gh" && text.startsWith("secret list")) return JSON.stringify([{ name: "KEY" }]);
    return "";
  });
  vi.spyOn(console, "info").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
  process.exitCode = 0;
  process.argv = ["bun", "scripts/release.ts", "0.8.4-beta"];
});
afterEach(() => {
  process.argv = originalArgv; process.exitCode = originalExit;
  rmSync(state.root, { recursive: true, force: true });
  vi.restoreAllMocks();
});
it("prepares a version and dispatches signed remote builds without local Rust or signing secrets", async () => {
  await import("./release");
  expect(process.exitCode).toBe(0);
  expect(JSON.parse(readFileSync(path.join(state.root, "package.json"), "utf8"))).toEqual({ version: "0.8.4-beta", scripts: { untouched: "value" } });
  expect(state.command).toHaveBeenCalledWith("git", ["push", "--atomic", "origin", "main", "refs/tags/v0.8.4-beta"]);
  expect(state.command).toHaveBeenCalledWith("gh", ["workflow", "run", "release-macos.yml", "--repo", "paulovnas/jarvis", "--ref", "main", "-f", "tag=v0.8.4-beta", "-f", "publish=true"]);
  expect(state.command.mock.calls.some(([program]) => ["cargo", "security", "codesign", "bun"].includes(String(program)))).toBe(false);
});
it("keeps dry-run entirely local and read-only", async () => {
  process.argv.push("--dry-run");
  await import("./release");
  expect(process.exitCode).toBe(0);
  expect(state.command).not.toHaveBeenCalled();
  expect(readFileSync(path.join(state.root, "package.json"), "utf8")).toContain("0.8.3-beta.1");
});
it("does not mutate version files or dispatch when a version is already public", async () => {
  state.optionalRelease.mockReturnValue({ isDraft: false });
  await import("./release");
  expect(process.exitCode).toBe(1);
  expect(state.command.mock.calls.some(([, args]) => (args as string[]).includes("push"))).toBe(false);
  expect(readFileSync(path.join(state.root, "package.json"), "utf8")).toContain("0.8.3-beta.1");
});
it("stops before version changes when signing secrets are not configured", async () => {
  const original = state.command.getMockImplementation()!;
  state.command.mockImplementation((program, args, ...rest) => program === "gh" && (args as string[]).join(" ").startsWith("secret list") ? "[]" : original(program, args, ...rest));
  await import("./release");
  expect(process.exitCode).toBe(1);
  expect(state.command.mock.calls.some(([, args]) => (args as string[]).includes("push"))).toBe(false);
  expect(readFileSync(path.join(state.root, "package.json"), "utf8")).toContain("0.8.3-beta.1");
});
