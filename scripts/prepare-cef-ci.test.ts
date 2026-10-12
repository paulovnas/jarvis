import { afterEach, expect, it, vi } from "vitest";
import { chmod, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { CEF_BINDINGS_VERSION, prepareCefCi } from "./prepare-cef-ci";

const directories: string[] = [];
afterEach(async () => { await Promise.all(directories.splice(0).map(directory => rm(directory, { recursive: true, force: true }))); });

async function fixture(platform: "darwin" | "win32" | "linux") {
  const home = await mkdtemp(path.join(tmpdir(), "Jarvis CEF CI "));
  directories.push(home);
  const envFile = path.join(home, "github-env");
  const pathFile = path.join(home, "github-path");
  const directory = path.join(home, ".cache", "jarvis", "cef", CEF_BINDINGS_VERSION, `${platform}-x64`);
  const binary = platform === "win32" ? "export-cef-dir.exe" : "export-cef-dir";
  const marker = path.join(directory, platform === "darwin" ? "Chromium Embedded Framework.framework/Chromium Embedded Framework" : platform === "win32" ? "libcef.dll" : "libcef.so");
  const run = vi.fn(async (program: string, args: string[]) => {
    if (program === "cargo") {
      const root = args[args.indexOf("--root") + 1];
      await mkdir(path.join(root, "bin"), { recursive: true });
      await writeFile(path.join(root, "bin", binary), "exporter fixture");
      await chmod(path.join(root, "bin", binary), 0o755);
      await writeFile(path.join(root, ".crates2.json"), JSON.stringify({ installs: { [`export-cef-dir ${CEF_BINDINGS_VERSION} (registry+https://github.com/rust-lang/crates.io-index)`]: { bins: [binary] } } }));
    } else {
      await mkdir(path.dirname(marker), { recursive: true });
      await writeFile(marker, "CEF native distribution fixture");
    }
  });
  return { home, envFile, pathFile, platform, arch: "x64", directory, marker, run, env: { LD_LIBRARY_PATH: "/existing/lib", DYLD_FALLBACK_LIBRARY_PATH: "/existing/mac" } };
}

it.each(["darwin", "win32", "linux"] as const)("exports pinned native CEF and loader paths for %s, then reuses a verified cache", async platform => {
  const f = await fixture(platform);
  expect(await prepareCefCi("/project with spaces", f)).toBe(f.directory);
  expect(f.run.mock.calls[0]).toEqual(["cargo", expect.arrayContaining(["install", "export-cef-dir", "--version", "=151.8.1", "--locked"]), expect.objectContaining({ cwd: "/project with spaces", env: expect.objectContaining({ CARGO_TARGET_DIR: path.join(f.home, ".cache", "jarvis", "cef-exporter", CEF_BINDINGS_VERSION, "target") }) })]);
  const variables = await readFile(f.envFile, "utf8");
  expect(variables).toContain(`CEF_PATH=${f.directory}\n`);
  if (platform === "linux") expect(variables).toContain(`LD_LIBRARY_PATH=${f.directory}:/existing/lib`);
  if (platform === "darwin") expect(variables).toContain(`DYLD_FALLBACK_LIBRARY_PATH=/existing/mac:${f.directory}`);
  if (platform === "win32") expect(await readFile(f.pathFile, "utf8")).toBe(`${f.directory}\n`);
  f.run.mockClear();
  await prepareCefCi("/project with spaces", f);
  expect(f.run).not.toHaveBeenCalled();
});

it("repairs an incomplete distribution before exporting it to the test linker", async () => {
  const f = await fixture("linux");
  await prepareCefCi("/project", f);
  await writeFile(f.marker, "");
  f.run.mockClear();
  await prepareCefCi("/project", f);
  expect(f.run).toHaveBeenCalledOnce();
  expect(f.run.mock.calls[0][0]).toContain("export-cef-dir");
  expect((await readFile(f.marker)).length).toBeGreaterThan(0);
});

it("rejects an unverified exporter instead of using a differently versioned native runtime", async () => {
  const f = await fixture("linux");
  f.run.mockImplementation(async () => {});
  await expect(prepareCefCi("/project", f)).rejects.toThrow("O exportador CEF instalado não corresponde à versão fixada.");
  await expect(readFile(f.envFile)).rejects.toMatchObject({ code: "ENOENT" });
});
