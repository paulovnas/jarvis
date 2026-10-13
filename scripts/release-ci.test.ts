import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { artifactNames, releaseTargets } from "./release-plan";
import { verifyReleaseArtifacts } from "./release-artifacts";

const state = vi.hoisted(() => ({ root: "", command: vi.fn(), optionalRelease: vi.fn<() => { isDraft: boolean; body: string } | null>() }));
vi.mock("./release-common", () => ({
  get root() { return state.root; }, command: state.command, optionalRelease: state.optionalRelease,
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
  state.optionalRelease.mockReset().mockReturnValue(null);
  vi.mocked(verifyReleaseArtifacts).mockReset();
  vi.stubEnv("RELEASE_TARGET", target);
  vi.stubEnv("RELEASE_TAG", "");
  vi.stubEnv("RELEASE_SHA", sha);
  vi.stubEnv("RELEASE_PUBLISH", "false");
  vi.stubEnv("RELEASE_SELECTION", "all");
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

it("rejects a Linux artifact before staging when the release selects only macOS", async () => {
  vi.stubEnv("RELEASE_SELECTION", "aarch64-apple-darwin");
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(verifyReleaseArtifacts).not.toHaveBeenCalled();
  expect(existsSync(path.join(state.root, "release-artifacts"))).toBe(false);
});

function preparePublication(contents: string, taggedSha = sha) {
  vi.stubEnv("RELEASE_TAG", "v1.4.0");
  vi.stubEnv("RELEASE_PUBLISH", "true");
  vi.stubEnv("GITHUB_REPOSITORY", "paulovnas/jarvis");
  vi.stubEnv("GITHUB_REF", "refs/heads/main");
  vi.stubEnv("GITHUB_OUTPUT", path.join(state.root, "output"));
  state.command.mockImplementation((program: string, args: string[]) => {
    if (program === "git" && ["cat-file", "for-each-ref"].includes(args[0])) return contents;
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
  expect(JSON.parse(readFileSync(path.join(state.root, "release-artifacts/installed-release.json"), "utf8"))).toEqual({ version: "1.4.0", notes: "Notas." });
});

it("packages all tag notes before a build and excludes the validation trailer", async () => {
  const notes = "## Novidades\n\n- Modal automática.\n\n## Correções\n\n- Disponível offline.";
  preparePublication(`Jarvis 1.4.0\n\n${notes}\n\nJarvis-Local-Checks-v1: ${sha}\n`);
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(JSON.parse(readFileSync(path.join(state.root, "release-artifacts/installed-release.json"), "utf8"))).toEqual({ version: "1.4.0", notes });
  expect(state.command.mock.calls.some(([program]) => program === "gh")).toBe(false);
  vi.resetModules();
  process.argv[2] = "stage";
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(JSON.parse(readFileSync(path.join(state.root, "release-artifacts/installed-release.json"), "utf8"))).toEqual({ version: "1.4.0", notes });
});

it("clears stale release notes when validating an untagged development build", async () => {
  const file = path.join(state.root, "release-artifacts/installed-release.json");
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, JSON.stringify({ version: "1.4.0", notes: "Notas antigas." }));
  vi.stubEnv("GITHUB_REPOSITORY", "paulovnas/jarvis");
  vi.stubEnv("GITHUB_REF", "refs/heads/main");
  process.argv[2] = "prepare";
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(JSON.parse(readFileSync(file, "utf8"))).toBeNull();
});

it("packages a validated legacy empty-note tag without generating different notes", async () => {
  preparePublication(`Jarvis 1.4.0\n\n\n\nJarvis-Local-Checks-v1: ${sha}\n`);
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(JSON.parse(readFileSync(path.join(state.root, "release-artifacts/installed-release.json"), "utf8"))).toEqual({ version: "1.4.0", notes: "" });
});

it.each(["missing", "different-validation", "different-tag"])("rejects an unvalidated or mismatched source before producing build outputs: %s", async reason => {
  const validation = reason === "missing" ? "" : `\n\nJarvis-Local-Checks-v1: ${reason === "different-validation" ? "d".repeat(40) : sha}`;
  preparePublication(`Jarvis 1.4.0\n\nNotas.${validation}\n`, reason === "different-tag" ? "e".repeat(40) : sha);
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(verifyReleaseArtifacts).not.toHaveBeenCalled();
  expect(existsSync(path.join(state.root, "release-artifacts/installed-release.json"))).toBe(false);
  expect(state.command.mock.calls.some(([program]) => program === "gh")).toBe(false);
});

function publicationWithNotes(notes: string, extraAssets: { name: string; size: number }[] = []) {
  preparePublication(`Jarvis 1.4.0\n\n${notes}\n\nJarvis-Local-Checks-v1: ${sha}\n`);
  process.argv[2] = "publish";
  const output = path.join(state.root, "release-artifacts");
  mkdirSync(output, { recursive: true });
  writeFileSync(path.join(output, "installed-release.json"), JSON.stringify({ version: "1.4.0", notes }));
  vi.mocked(verifyReleaseArtifacts).mockImplementation((directory, version, _sha, releaseTarget) => {
    const names = artifactNames(version, releaseTarget);
    for (const name of names.names) writeFileSync(path.join(directory, name), "artifact");
    return { version, target: releaseTarget, archive: names.archive, signature: "signature", names: names.names };
  });
  state.optionalRelease.mockReturnValue({ isDraft: true, body: notes });
  const original = state.command.getMockImplementation()!;
  state.command.mockImplementation((program: string, args: string[]) => {
    if (program === "gh" && args[0] === "release" && args[1] === "view") {
      const upload = state.command.mock.calls.find(([tool, input]) => tool === "gh" && (input as string[])[1] === "upload");
      const files = (upload?.[1] as string[] | undefined)?.filter(file => file.startsWith(`${output}${path.sep}`)) ?? [];
      return JSON.stringify({ isDraft: true, assets: [...files.map(file => ({ name: path.basename(file), size: statSync(file).size })), ...extraAssets] });
    }
    return original(program, args);
  });
  return output;
}

it.each(["## Novidades\n\n- Alteração completa.\n\n## Correções\n\n- Última correção.", ""])("publishes canonical notes unchanged in the draft and every updater manifest: %j", async notes => {
  const output = publicationWithNotes(notes);
  state.optionalRelease.mockReturnValueOnce(null);
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(state.command).toHaveBeenCalledWith("gh", ["release", "create", "v1.4.0", "--repo", "paulovnas/jarvis", "--verify-tag", "--draft", "--title", "Jarvis 1.4.0", "--notes-file", path.join(output, "notes.md")]);
  expect(readFileSync(path.join(output, "notes.md"), "utf8")).toBe(notes);
  for (const platform of ["latest.json", "latest-darwin-aarch64.json", "latest-windows-x86_64.json", "latest-linux-x86_64.json"]) {
    expect(JSON.parse(readFileSync(path.join(output, platform), "utf8")).notes).toBe(notes);
  }
  expect(verifyReleaseArtifacts).toHaveBeenCalledTimes(releaseTargets.length);
  expect(state.command.mock.calls.some(([, args]) => (args as string[]).includes("--generate-notes"))).toBe(false);
  expect(state.command).toHaveBeenCalledWith("gh", ["release", "edit", "v1.4.0", "--repo", "paulovnas/jarvis", "--draft=false", "--latest=true"]);
});

it("verifies and uploads only macOS packages and updater manifests for a selected release", async () => {
  const output = publicationWithNotes("Somente macOS.");
  vi.stubEnv("RELEASE_SELECTION", "aarch64-apple-darwin");
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(verifyReleaseArtifacts).toHaveBeenCalledTimes(1);
  expect(verifyReleaseArtifacts).toHaveBeenCalledWith(output, "1.4.0", sha, "aarch64-apple-darwin", "test-public-key");
  expect(Object.keys(JSON.parse(readFileSync(path.join(output, "latest.json"), "utf8")).platforms)).toEqual(["darwin-aarch64"]);
  expect(Object.keys(JSON.parse(readFileSync(path.join(output, "latest-darwin-aarch64.json"), "utf8")).platforms)).toEqual(["darwin-aarch64"]);
  expect(readdirSync(output).some(name => /windows|linux|\.exe|\.deb|\.AppImage/.test(name))).toBe(false);
  expect(state.command).toHaveBeenCalledWith("gh", ["release", "upload", "v1.4.0", "--repo", "paulovnas/jarvis", "--clobber", ...[...artifactNames("1.4.0", "aarch64-apple-darwin").names, "latest.json", "latest-darwin-aarch64.json"].map(name => path.join(output, name))]);
  expect(state.command).toHaveBeenCalledWith("gh", ["release", "edit", "v1.4.0", "--repo", "paulovnas/jarvis", "--draft=false", "--latest=true"]);
});

it("publishes the two selected platforms and their updater manifests together", async () => {
  const output = publicationWithNotes("macOS e Linux.");
  vi.stubEnv("RELEASE_SELECTION", "macos-linux");
  await import("./release-ci");
  expect(process.exitCode).toBe(0);
  expect(verifyReleaseArtifacts).toHaveBeenCalledTimes(2);
  expect(verifyReleaseArtifacts).toHaveBeenCalledWith(output, "1.4.0", sha, "x86_64-unknown-linux-gnu", "test-public-key");
  expect(Object.keys(JSON.parse(readFileSync(path.join(output, "latest.json"), "utf8")).platforms)).toEqual(["darwin-aarch64", "linux-x86_64"]);
  expect(Object.keys(JSON.parse(readFileSync(path.join(output, "latest-linux-x86_64.json"), "utf8")).platforms)).toEqual(["linux-x86_64"]);
  expect(readdirSync(output).some(name => /windows|\.exe/.test(name))).toBe(false);
  expect(state.command).toHaveBeenCalledWith("gh", ["release", "edit", "v1.4.0", "--repo", "paulovnas/jarvis", "--draft=false", "--latest=true"]);
});

it("keeps a combined release private when an unselected Windows package remains in the draft", async () => {
  publicationWithNotes("macOS e Linux.", [{ name: "Jarvis_1.4.0_x86_64-pc-windows-msvc-setup.exe", size: 100 }]);
  vi.stubEnv("RELEASE_SELECTION", "macos-linux");
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(state.command.mock.calls.some(([program, args]) => program === "gh" && (args as string[])[1] === "edit")).toBe(false);
  expect(console.error).toHaveBeenCalledWith(expect.stringContaining("fora da seleção de plataformas"));
});

it.each(["Jarvis_1.4.0_x86_64-pc-windows-msvc-setup.exe", "latest-linux-x86_64.json"])("keeps a macOS-only draft private when it contains an unselected asset: %s", async name => {
  publicationWithNotes("Somente macOS.", [{ name, size: 100 }]);
  vi.stubEnv("RELEASE_SELECTION", "aarch64-apple-darwin");
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(state.command.mock.calls.some(([program, args]) => program === "gh" && (args as string[])[1] === "edit")).toBe(false);
  expect(console.error).toHaveBeenCalledWith(expect.stringContaining("fora da seleção de plataformas"));
});

it("keeps the default full-platform release private if a required platform is unavailable", async () => {
  publicationWithNotes("Todas as plataformas.");
  vi.mocked(verifyReleaseArtifacts).mockImplementationOnce((directory, version, _sha, releaseTarget) => {
    const names = artifactNames(version, releaseTarget);
    return { version, target: releaseTarget, archive: names.archive, signature: "signature", names: names.names };
  }).mockImplementationOnce(() => { throw new Error("Windows package missing"); });
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(state.command.mock.calls.some(([program]) => program === "gh")).toBe(false);
});

it("rejects an unknown selection before checking out a tag or creating package metadata", async () => {
  preparePublication(`Jarvis 1.4.0\n\nNotas.\n\nJarvis-Local-Checks-v1: ${sha}\n`);
  vi.stubEnv("RELEASE_SELECTION", "unknown");
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(state.command).not.toHaveBeenCalled();
  expect(existsSync(path.join(state.root, "release-artifacts"))).toBe(false);
});

it.each(["missing", "different-version", "different-notes", "edited-draft"])("keeps the release private when the canonical notes disagree with the package: %s", async reason => {
  const output = publicationWithNotes("Notas originais.");
  const file = path.join(output, "installed-release.json");
  if (reason === "missing") rmSync(file);
  if (reason === "different-version") writeFileSync(file, JSON.stringify({ version: "1.5.0", notes: "Notas originais." }));
  if (reason === "different-notes") writeFileSync(file, JSON.stringify({ version: "1.4.0", notes: "Notas diferentes." }));
  if (reason === "edited-draft") state.optionalRelease.mockReturnValue({ isDraft: true, body: "Notas diferentes." });
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(state.command.mock.calls.some(([program, args]) => program === "gh" && ["upload", "edit"].includes((args as string[])[1]))).toBe(false);
});

it("cannot bypass local validation by invoking publication directly", async () => {
  preparePublication("Jarvis 1.4.0\n\nNotas.\n");
  process.argv[2] = "publish";
  await import("./release-ci");
  expect(process.exitCode).toBe(1);
  expect(verifyReleaseArtifacts).not.toHaveBeenCalled();
  expect(state.command.mock.calls.some(([program]) => program === "gh")).toBe(false);
});
