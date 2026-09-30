import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import * as installedRelease from "./installed-release";
import { readInstalledRelease } from "./installed-release";

vi.mock("vite", () => ({ defineConfig: (configuration: unknown) => configuration }));
vi.mock("@vitejs/plugin-react", () => ({ default: () => [] }));
vi.mock("@tailwindcss/vite", () => ({ default: () => [] }));

let directory: string;
let file: string;
beforeEach(() => {
  directory = mkdtempSync(path.join(tmpdir(), "jarvis-installed-release-"));
  file = path.join(directory, "installed-release.json");
});
afterEach(() => {
  rmSync(directory, { recursive: true, force: true });
  vi.restoreAllMocks();
});

it("keeps builds without release metadata silent", () => {
  expect(readInstalledRelease(file)).toBeNull();
  writeFileSync(file, "null\n");
  expect(readInstalledRelease(file)).toBeNull();
});
it("embeds the exact prerelease version and full Markdown without truncation", () => {
  const release = { version: "1.8.5-beta.2", notes: `## Novidades\n\n${"- Alteração completa.\n".repeat(5000)}\n## Correções\n\n- Última correção.` };
  writeFileSync(file, JSON.stringify(release) + "\n");
  expect(readInstalledRelease(file)).toEqual(release);
});
it.each(["broken-json", '{}', '{"version":"1.8.5","notes":42}'])("fails packaging instead of accepting invalid release metadata: %s", contents => {
  writeFileSync(file, contents);
  expect(() => readInstalledRelease(file)).toThrow();
});

it("defines installed notes in production bundles and keeps the development server silent", async () => {
  const release = { version: "1.8.5-beta.2", notes: "## Novidades\n\n- Notas locais." };
  const read = vi.spyOn(installedRelease, "readInstalledRelease").mockReturnValue(release);
  const { default: configure } = await import("../vite.config");
  if (typeof configure !== "function") throw new Error("Expected the Vite configuration function");
  const build = await configure({ command: "build", mode: "production" });
  expect(build.define?.__JARVIS_INSTALLED_RELEASE__).toBe(JSON.stringify(release));
  expect(read).toHaveBeenCalledWith(path.join(process.cwd(), "release-artifacts/installed-release.json"));
  read.mockClear();
  const development = await configure({ command: "serve", mode: "development" });
  expect(development.define?.__JARVIS_INSTALLED_RELEASE__).toBe("null");
  expect(read).not.toHaveBeenCalled();
});
