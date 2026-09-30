import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  files: [] as string[], before: {} as Record<string, string>, after: {} as Record<string, string>,
  command: vi.fn<(program: string, args: string[], capture?: boolean) => string>(),
}));
vi.mock("./release-common", () => ({
  command: state.command,
  versionFiles: ["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"],
  read: (file: string) => state.after[file],
}));

const base = "d".repeat(40);
const originalExit = process.exitCode;
let directory: string;
let output: string;
beforeEach(() => {
  vi.resetModules();
  directory = mkdtempSync(path.join(tmpdir(), "jarvis-native-changes-"));
  output = path.join(directory, "output");
  state.before = {
    "package.json": '{"version":"1.8.2","scripts":{"test":"vitest run"}}',
    "src-tauri/tauri.conf.json": '{"version":"1.8.2","productName":"Jarvis"}',
    "src-tauri/Cargo.toml": '[package]\nname = "jarvis"\nversion = "1.8.2"\n\n[dependencies]\nserde = "1"\n',
    "src-tauri/Cargo.lock": '[[package]]\nname = "jarvis"\nversion = "1.8.2"\n\n[[package]]\nname = "serde"\nversion = "1.0.0"\n',
  };
  state.after = Object.fromEntries(Object.entries(state.before).map(([file, contents]) => [file, contents.replace("1.8.2", "1.8.3")]));
  state.files = Object.keys(state.before);
  state.command.mockReset().mockImplementation((_program, args) => args[0] === "diff" ? state.files.join("\n") : state.before[args[1].slice(base.length + 1)].trim());
  vi.stubEnv("NATIVE_BASE_SHA", base);
  vi.stubEnv("GITHUB_EVENT_NAME", "push");
  vi.stubEnv("GITHUB_OUTPUT", output);
  vi.spyOn(console, "info").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
  process.exitCode = 0;
});
afterEach(() => {
  process.exitCode = originalExit;
  vi.unstubAllEnvs(); vi.restoreAllMocks();
  rmSync(directory, { recursive: true, force: true });
});

it("skips only a version-only change across all four release manifests", async () => {
  await import("./native-validation");
  expect(process.exitCode).toBe(0);
  expect(readFileSync(output, "utf8")).toBe("required=false\n");
});

it.each(["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/src/agent/mod.rs"])("keeps native checks for meaningful changes: %s", async file => {
  if (file.endsWith(".json")) state.after[file] = state.after[file].replace(/}$/, ',"newSetting":true}');
  else if (state.after[file]) state.after[file] = state.after[file].replace('serde = "1"', 'serde = "2"').replace('version = "1.0.0"', 'version = "2.0.0"');
  else state.files.push(file);
  await import("./native-validation");
  expect(process.exitCode).toBe(0);
  expect(readFileSync(output, "utf8")).toBe("required=true\n");
});

it.each(["manual", "unknown-base"])("runs the native suites when validation is explicitly requested or the base is unknown: %s", async reason => {
  if (reason === "manual") vi.stubEnv("GITHUB_EVENT_NAME", "workflow_dispatch");
  else vi.stubEnv("NATIVE_BASE_SHA", "0".repeat(40));
  await import("./native-validation");
  expect(process.exitCode).toBe(0);
  expect(readFileSync(output, "utf8")).toBe("required=true\n");
});

it("does not declare validation unnecessary when the source comparison fails", async () => {
  state.command.mockImplementation(() => { throw new Error("base is unavailable"); });
  await import("./native-validation");
  expect(process.exitCode).toBe(1);
});
