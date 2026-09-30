import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { artifactNames } from "./release-plan";
import { verifyReleaseArtifacts } from "./release-artifacts";

const state = vi.hoisted(() => ({ root: "", command: vi.fn() }));
vi.mock("./release-common", () => ({
  get root() { return state.root; }, command: state.command, optionalRelease: vi.fn(),
  configuration: () => ({ config: { version: "1.4.0", productName: "Jarvis", plugins: { updater: { pubkey: "test-public-key" } } } }),
}));
vi.mock("./release-artifacts", () => ({ verifyReleaseArtifacts: vi.fn() }));
const target = "x86_64-unknown-linux-gnu";
const originalArgv = process.argv;
const originalExit = process.exitCode;
const sha = "c".repeat(40);
let bundle: string;
beforeEach(() => {
  vi.resetModules();
  state.root = mkdtempSync(path.join(tmpdir(), "jarvis-linux-release-"));
  bundle = path.join(state.root, "src-tauri/target", target, "release/bundle");
  mkdirSync(path.join(bundle, "appimage"), { recursive: true });
  mkdirSync(path.join(bundle, "deb"), { recursive: true });
  writeFileSync(path.join(bundle, "appimage/Jarvis_1.4.0_amd64.AppImage"), "signed-appimage");
  writeFileSync(path.join(bundle, "appimage/Jarvis_1.4.0_amd64.AppImage.sig"), "signature");
  writeFileSync(path.join(bundle, "deb/Jarvis_1.4.0_amd64.deb"), "deb-installer");
  state.command.mockReset().mockReturnValue(sha);
  vi.mocked(verifyReleaseArtifacts).mockReset();
  vi.stubEnv("RELEASE_TARGET", target);
  vi.stubEnv("RELEASE_TAG", "");
  vi.stubEnv("RELEASE_SHA", sha);
  vi.stubEnv("RELEASE_PUBLISH", "false");
  vi.spyOn(console, "info").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
  process.argv = ["bun", "scripts/release-ci.ts", "stage"];
  process.exitCode = 0;
});
afterEach(() => {
  process.argv = originalArgv; process.exitCode = originalExit;
  vi.unstubAllEnvs(); vi.restoreAllMocks();
  rmSync(state.root, { recursive: true, force: true });
});

it("stages the Linux installer and signed AppImage without invoking Apple signing tools", async () => {
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  const output = path.join(state.root, "release-artifacts");
  const names = artifactNames("1.4.0", target);
  expect(readFileSync(path.join(output, names.archive), "utf8")).toBe("signed-appimage");
  expect(readFileSync(path.join(output, names.signature), "utf8")).toBe("signature");
  expect(readFileSync(path.join(output, names.names[0]), "utf8")).toBe("deb-installer");
  expect(JSON.parse(readFileSync(path.join(output, `build-${target}.json`), "utf8"))).toEqual({ version: "1.4.0", sha, target });
  expect(verifyReleaseArtifacts).toHaveBeenCalledWith(output, "1.4.0", sha, target, "test-public-key");
  expect(state.command.mock.calls.every(([program]) => program === "git")).toBe(true);
});

it.each(["signature", "ambiguous", "missing-deb"])("rejects incomplete or ambiguous Linux artifacts: %s", async failure => {
  if (failure === "signature") rmSync(path.join(bundle, "appimage/Jarvis_1.4.0_amd64.AppImage.sig"));
  if (failure === "ambiguous") writeFileSync(path.join(bundle, "appimage/Jarvis_1.4.0_other.AppImage"), "other");
  if (failure === "missing-deb") rmSync(path.join(bundle, "deb/Jarvis_1.4.0_amd64.deb"));
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(verifyReleaseArtifacts).not.toHaveBeenCalled();
});

function preparePublication(contents: string, taggedSha = sha) {
  vi.stubEnv("RELEASE_TAG", "v1.4.0");
  vi.stubEnv("RELEASE_PUBLISH", "true");
  vi.stubEnv("GITHUB_REPOSITORY", "paulovnas/jarvis");
  vi.stubEnv("GITHUB_REF", "refs/heads/main");
  vi.stubEnv("GITHUB_OUTPUT", path.join(state.root, "output"));
  state.command.mockImplementation((program: string, args: string[]) => {
    if (program === "git" && args[0] === "cat-file") return contents;
    if (program === "git" && args[0] === "rev-parse") return args[1] === "HEAD" ? sha : taggedSha;
    return "";
  });
  process.argv[2] = "prepare";
}

it("prepares the selected tag using the current launcher's exact-commit validation contract", async () => {
  preparePublication(`Jarvis 1.4.0\n\nNotas.\n\nJarvis-Local-Checks-v1: ${sha}\n`);
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(state.command).toHaveBeenCalledWith("git", ["merge-base", "--is-ancestor", sha, "HEAD"], true);
  expect(state.command).toHaveBeenCalledWith("git", ["checkout", "--detach", sha]);
  expect(readFileSync(path.join(state.root, "output"), "utf8")).toBe(`sha=${sha}\nversion=1.4.0\n`);
});

it.each(["missing", "different-validation", "different-tag"])("rejects an unvalidated or mismatched source before producing build outputs: %s", async reason => {
  const validation = reason === "missing" ? "" : `\n\nJarvis-Local-Checks-v1: ${reason === "different-validation" ? "d".repeat(40) : sha}`;
  preparePublication(`Jarvis 1.4.0\n\nNotas.${validation}\n`, reason === "different-tag" ? "e".repeat(40) : sha);
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(verifyReleaseArtifacts).not.toHaveBeenCalled();
  expect(state.command.mock.calls.some(([program]) => program === "gh")).toBe(false);
});

it("cannot bypass local validation by invoking publication directly", async () => {
  preparePublication("Jarvis 1.4.0\n\nNotas.\n");
  process.argv[2] = "publish";
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(verifyReleaseArtifacts).not.toHaveBeenCalled();
  expect(state.command.mock.calls.some(([program]) => program === "gh")).toBe(false);
});
