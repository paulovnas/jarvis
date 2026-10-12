import { execFileSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import { ensureCefTauriCli, TAURI_CEF_REPOSITORY, TAURI_CEF_REVISION } from "./tauri-cef-cli";

const temporary: string[] = [];

it("keeps the embedded runtime, bundler and window ABI on compatible pinned sources", async () => {
  const cargo = await readFile(path.resolve("src-tauri/Cargo.toml"), "utf8");
  for (const name of ["tauri-runtime-cef", "tauri", "tauri-build", "tauri-codegen", "tauri-macros", "tauri-runtime", "tauri-utils", "tauri-plugin"]) {
    expect(cargo).toContain(`${name} = { git = "${TAURI_CEF_REPOSITORY}", rev = "${TAURI_CEF_REVISION}"`);
  }
  const lock = await readFile(path.resolve("src-tauri/Cargo.lock"), "utf8");
  const windowSources = lock.split("[[package]]").filter(block => /^\s*name = "(?:winit(?:-[^"]+)?|dpi)"/m.test(block));
  expect(windowSources.length).toBeGreaterThan(1);
  for (const block of windowSources) expect(block).toContain("git+https://github.com/tauri-apps/winit-gtk4?branch=master#3d7c7cd19d3c752f75762ceea535444e63bdf8de");
  const config = JSON.parse(await readFile(path.resolve("src-tauri/tauri.conf.json"), "utf8")) as { bundle: { cef: { embed: boolean }; macOS: { minimumSystemVersion: string } } };
  expect(config.bundle.cef.embed).toBe(true);
  // The pinned CEF framework's Mach-O LC_BUILD_VERSION requires macOS 13.
  expect(Number.parseFloat(config.bundle.macOS.minimumSystemVersion)).toBeGreaterThanOrEqual(13);
});
afterEach(async () => { await Promise.all(temporary.splice(0).map(directory => rm(directory, { recursive: true, force: true }))); });

async function cache() {
  const directory = await mkdtemp(path.join(tmpdir(), "Jarvis CEF CLI "));
  temporary.push(directory);
  return directory;
}

async function installed(directory: string, revision = TAURI_CEF_REVISION, binary = "cargo-tauri") {
  await mkdir(path.join(directory, "bin"), { recursive: true });
  await writeFile(path.join(directory, "bin", binary), '#!/bin/sh\nprintf \'%s\\n\' "$@"\n');
  await chmod(path.join(directory, "bin", binary), 0o755);
  await writeFile(path.join(directory, ".crates2.json"), JSON.stringify({ installs: {
    [`tauri-cli 2.11.4 (git+${TAURI_CEF_REPOSITORY}?rev=${revision}#${revision})`]: { bins: [binary] },
  } }));
}

it("reuses the matching revision and preserves native CLI arguments including spaces", async () => {
  const directory = await cache();
  await installed(directory);
  const installer = vi.fn();
  const cli = await ensureCefTauriCli("/project with spaces", { directory, installer, platform: "darwin" });
  expect(cli).toBe(path.join(directory, "bin", "cargo-tauri"));
  expect(installer).not.toHaveBeenCalled();
  if (process.platform !== "win32") expect(execFileSync(cli, ["build", "--config", "config with spaces.json", "--", "--locked"], { encoding: "utf8" }).trim().split("\n")).toEqual(["build", "--config", "config with spaces.json", "--", "--locked"]);
});

it.each([undefined, "old-revision"])("installs the exact CLI and bundler revision when cache provenance is %s", async revision => {
  const directory = await cache();
  if (revision) await installed(directory, revision);
  const installer = vi.fn(async () => { await installed(directory); });
  expect(await ensureCefTauriCli("/project with spaces", { directory, installer, platform: "darwin" })).toBe(path.join(directory, "bin", "cargo-tauri"));
  expect(installer).toHaveBeenCalledExactlyOnceWith("cargo", ["install", "--git", TAURI_CEF_REPOSITORY, "--rev", TAURI_CEF_REVISION, "--locked", "--root", directory, "--bin", "cargo-tauri", "--force", "tauri-cli"], {
    cwd: "/project with spaces", env: expect.objectContaining({ CARGO_TARGET_DIR: path.join(directory, "target") }),
  });
});

it("rejects an installed executable with no matching Cargo provenance rather than falling back to the published CLI", async () => {
  const directory = await cache();
  const installer = vi.fn(async () => { await installed(directory, "wrong-revision"); });
  await expect(ensureCefTauriCli("/project", { directory, installer, platform: "darwin" })).rejects.toThrow("CLI Tauri padrão não será utilizado");
  expect(installer).toHaveBeenCalledOnce();
});

it("keeps the Windows executable name while using the same source revision", async () => {
  const directory = await cache();
  await installed(directory, TAURI_CEF_REVISION, "cargo-tauri.exe");
  const installer = vi.fn();
  expect(await ensureCefTauriCli("/project", { directory, installer, platform: "win32" })).toBe(path.join(directory, "bin", "cargo-tauri.exe"));
  expect(installer).not.toHaveBeenCalled();
});
