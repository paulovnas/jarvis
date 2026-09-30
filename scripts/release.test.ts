import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  root: "", command: vi.fn<(...args: unknown[]) => string>(),
  optionalRelease: vi.fn<() => { isDraft: boolean } | null>(),
  head: "", remote: "", tagCommit: null as string | null, tagContents: "", dirty: "", branch: "main",
}));
vi.mock("./release-common", () => ({
  get root() { return state.root; },
  versionFiles: ["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"],
  releaseWorkflow: "release-macos.yml", releaseEnvironment: "macos-release",
  requiredSecrets: ["KEY"], command: state.command, optionalRelease: state.optionalRelease,
  configuration: () => {
    const pkg = JSON.parse(readFileSync(path.join(state.root, "package.json"), "utf8")) as { version: string };
    return { pkg, config: { version: pkg.version } };
  },
  read: (name: string) => readFileSync(path.join(state.root, name), "utf8"),
}));

const originalArgv = process.argv;
const originalExit = process.exitCode;
const originalSha = "a".repeat(40);
const releaseSha = "b".repeat(40);
const generatedNotes = "## Novidades\n\n- Ferramentas melhoradas.\n\n## Correções\n\n- Retomada estável.";
beforeEach(() => {
  vi.resetModules();
  state.root = mkdtempSync(path.join(tmpdir(), "jarvis-launcher-test-"));
  mkdirSync(path.join(state.root, "src-tauri"));
  writeFileSync(path.join(state.root, "package.json"), JSON.stringify({ version: "0.8.3-beta.1", scripts: { untouched: "value" } }));
  writeFileSync(path.join(state.root, "src-tauri/Cargo.toml"), '[package]\nname = "jarvis"\nversion = "0.8.3-beta.1"\n');
  writeFileSync(path.join(state.root, "src-tauri/Cargo.lock"), '[[package]]\nname = "jarvis"\nversion = "0.8.3-beta.1"\n');
  state.head = originalSha; state.remote = originalSha; state.tagCommit = null; state.tagContents = ""; state.dirty = ""; state.branch = "main";
  state.optionalRelease.mockReset().mockReturnValue(null);
  state.command.mockReset().mockImplementation((program, input) => {
    const args = input as string[];
    const text = args.join(" ");
    if (program === "git") {
      if (text === "branch --show-current") return state.branch;
      if (text === "status --porcelain") return state.dirty;
      if (text === "rev-parse HEAD") return state.head;
      if (text === "rev-parse origin/main" || text === "rev-parse HEAD^") return state.remote;
      if (text.startsWith("tag --list")) return state.tagCommit ? "v0.8.4-beta" : "";
      if (text.startsWith("rev-list")) return state.tagCommit ?? "";
      if (text.startsWith("cat-file tag")) return state.tagContents;
      if (text === "log -1 --format=%s") return "chore(release): v0.8.4-beta";
      if (args[0] === "commit") state.head = releaseSha;
      if (args[0] === "tag" && args.includes("--file")) {
        state.tagCommit = state.head;
        state.tagContents = readFileSync(args[args.indexOf("--file") + 1], "utf8");
      }
    }
    if (program === "gh" && text.startsWith("repo view")) return JSON.stringify({ nameWithOwner: "paulovnas/jarvis", isPrivate: false });
    if (program === "gh" && text.startsWith("secret list")) return JSON.stringify([{ name: "KEY" }]);
    if (program === "gh" && args[0] === "api") return generatedNotes;
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
it("validates the final version commit locally before tagging and dispatching signed remote builds", async () => {
  await import("./release");
  expect(process.exitCode).toBe(0);
  expect(JSON.parse(readFileSync(path.join(state.root, "package.json"), "utf8"))).toEqual({ version: "0.8.4-beta", scripts: { untouched: "value" } });
  expect(state.command).toHaveBeenCalledWith("git", ["push", "--atomic", "origin", `${releaseSha}:refs/heads/main`, "refs/tags/v0.8.4-beta"]);
  expect(state.command).toHaveBeenCalledWith("gh", ["workflow", "run", "release-macos.yml", "--repo", "paulovnas/jarvis", "--ref", "main", "-f", "tag=v0.8.4-beta", "-f", "publish=true"]);
  const calls = state.command.mock.calls.map(([program, args]) => `${program} ${(args as string[]).join(" ")}`);
  const ordered = ["gh api", "git commit", "bun install --frozen-lockfile", "bun run check", "cargo clippy", "cargo test", "git tag --no-sign -a", "git push", "gh workflow run"].map(prefix => calls.findIndex(call => call.startsWith(prefix)));
  expect(ordered.every((index, i) => index >= 0 && (i === 0 || index > ordered[i - 1]))).toBe(true);
  expect(state.tagContents).toContain(`Jarvis-Local-Checks-v1: ${releaseSha}`);
  expect(state.tagContents).toContain(generatedNotes);
  expect(state.command).toHaveBeenCalledWith("gh", ["api", "repos/paulovnas/jarvis/releases/generate-notes", "--method", "POST", "-f", "tag_name=v0.8.4-beta", "-f", `target_commitish=${originalSha}`, "--jq", ".body"], true);
  expect(state.command.mock.calls.some(([program]) => ["security", "codesign"].includes(String(program)))).toBe(false);
});
it("preserves complete custom notes in the tag without generating a different changelog", async () => {
  const file = path.join(state.root, "custom-notes.md");
  const notes = "## Novidades\n\n- Modal da versão.\n\n## Correções\n\n- Notas disponíveis offline.";
  writeFileSync(file, `\n${notes}\n`);
  process.argv.push("--notes-file", file);
  await import("./release");
  expect(process.exitCode).toBe(0);
  expect(state.tagContents).toContain(`\n\n${notes}\n\nJarvis-Local-Checks-v1: ${releaseSha}`);
  expect(state.command).toHaveBeenCalledWith("git", expect.arrayContaining(["tag", "--cleanup=verbatim"]));
  expect(state.command.mock.calls.some(([program, args]) => program === "gh" && (args as string[])[0] === "api")).toBe(false);
});
it("stops before changing the version when automatic notes generation fails", async () => {
  const original = state.command.getMockImplementation()!;
  state.command.mockImplementation((program, args, ...rest) => {
    if (program === "gh" && (args as string[])[0] === "api") throw new Error("notes request failed");
    return original(program, args, ...rest);
  });
  await import("./release");
  expect(process.exitCode).toBe(1);
  expect(readFileSync(path.join(state.root, "package.json"), "utf8")).toContain("0.8.3-beta.1");
  expect(state.tagCommit).toBeNull();
  expect(state.command.mock.calls.some(([, args]) => ["commit", "push"].includes((args as string[])[0]))).toBe(false);
});
it.each(["custom", "generated"])("rejects blank %s notes before changing files, validating or publishing a new tag", async source => {
  const files = ["package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"];
  const originals = files.map(file => readFileSync(path.join(state.root, file), "utf8"));
  if (source === "custom") {
    const file = path.join(state.root, "empty-notes.md");
    writeFileSync(file, " \n\t\n");
    process.argv.push("--notes-file", file);
  } else {
    const original = state.command.getMockImplementation()!;
    state.command.mockImplementation((program, args, ...rest) => program === "gh" && (args as string[])[0] === "api" ? " \n\t\n" : original(program, args, ...rest));
  }
  await import("./release");
  expect(process.exitCode).toBe(1);
  expect(files.map(file => readFileSync(path.join(state.root, file), "utf8"))).toEqual(originals);
  expect(existsSync(path.join(state.root, "src-tauri/tauri.conf.json"))).toBe(false);
  expect(state.tagCommit).toBeNull();
  expect(state.command.mock.calls.some(([program, input]) => {
    const args = input as string[];
    return ["bun", "cargo"].includes(String(program)) || ["commit", "push"].includes(args[0]) || (args[0] === "tag" && args.includes("--file")) || (program === "gh" && ["run", "create", "upload", "edit"].includes(args[1]));
  })).toBe(false);
});
it("uses the current launcher to select a validated release source without repeating full gates in packaging", () => {
  const workflow = readFileSync(path.join(process.cwd(), ".github/workflows/release-macos.yml"), "utf8");
  expect(workflow).toContain("ref: ${{ github.sha }}");
  expect(workflow).toContain("bun scripts/release-ci.ts prepare");
  for (const check of ["bun run check", "cargo clippy", "cargo test", "check-linux-keyring.sh"]) expect(workflow).not.toContain(check);
  expect(workflow).toContain("bun scripts/release-ci.ts stage");
  const native = readFileSync(path.join(process.cwd(), ".github/workflows/native-validation.yml"), "utf8");
  for (const required of ["src-tauri/**", "bun.lock", "package.json", "macos-15", "windows-2022", "ubuntu-22.04", "bun scripts/native-validation.ts", "needs.changes.outputs.required == 'true'", "check-linux-keyring.sh"]) expect(native).toContain(required);
  expect(native).toContain("--lib -- system:: secrets::");
  expect(native).toContain("bun run build:extension");
  expect(native).toContain("github.event_name == 'pull_request' && github.ref || github.sha");
  expect(native).toContain("cancel-in-progress: ${{ github.event_name == 'pull_request' }}");
  expect(native).not.toContain("bun run check");
  expect(native).not.toContain("secrets.");
});
it("allows enough time to upload every signed release artifact", () => {
  const workflow = readFileSync(path.join(process.cwd(), ".github/workflows/release-macos.yml"), "utf8");
  const publishJob = workflow.split("\n  publish:")[1];
  expect(publishJob).toBeDefined();
  expect(publishJob).toContain("timeout-minutes: 30");
});
it("prepares and transports installed release notes for every platform before publication", () => {
  const workflow = readFileSync(path.join(process.cwd(), ".github/workflows/release-macos.yml"), "utf8");
  const [build, publication] = workflow.split("\n  publish:");
  const prepare = build.indexOf("bun scripts/release-ci.ts prepare");
  expect(prepare).toBeGreaterThan(0);
  for (const target of ["aarch64-apple-darwin", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"]) {
    expect(build.indexOf(`--target ${target}`)).toBeGreaterThan(prepare);
  }
  expect(build.lastIndexOf("actions/checkout@")).toBeLessThan(prepare);
  expect(build).toContain("path: release-artifacts/");
  expect(publication).toContain("merge-multiple: true");
  expect(publication).toContain("path: release-artifacts");
  const download = publication.indexOf("actions/download-artifact@");
  expect(download).toBeGreaterThan(publication.lastIndexOf("actions/checkout@"));
  expect(publication.indexOf("bun scripts/release-ci.ts publish")).toBeGreaterThan(download);
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

it.each(["bun install", "bun run check", "cargo clippy", "cargo test"])("sends no tag or release when the local gate fails: %s", async gate => {
  const original = state.command.getMockImplementation()!;
  state.command.mockImplementation((program, args, ...rest) => {
    if (`${program} ${(args as string[]).join(" ")}`.startsWith(gate)) throw new Error("local gate failed");
    return original(program, args, ...rest);
  });
  await import("./release");
  expect(process.exitCode).toBe(1);
  expect(state.tagCommit).toBeNull();
  expect(state.command.mock.calls.some(([program, input]) => {
    const args = input as string[];
    return args[0] === "push" || (program === "gh" && args[0] === "workflow" && args[1] === "run");
  })).toBe(false);
});

it.each(["commit", "working tree", "branch"])("rejects source changes during local checks: %s", async changed => {
  const original = state.command.getMockImplementation()!;
  state.command.mockImplementation((program, args, ...rest) => {
    const output = original(program, args, ...rest);
    if (program === "cargo" && (args as string[])[0] === "test") {
      if (changed === "commit") state.head = originalSha;
      if (changed === "working tree") state.dirty = " M src/App.tsx";
      if (changed === "branch") state.branch = "other";
    }
    return output;
  });
  await import("./release");
  expect(process.exitCode).toBe(1);
  expect(state.tagCommit).toBeNull();
  expect(state.command.mock.calls.some(([, args]) => (args as string[])[0] === "push")).toBe(false);
});

it("retries the unchanged local version commit after checks failed before tag creation", async () => {
  writeFileSync(path.join(state.root, "package.json"), JSON.stringify({ version: "0.8.4-beta" }));
  state.head = releaseSha;
  await import("./release");
  expect(process.exitCode).toBe(0);
  expect(state.command.mock.calls.some(([, args]) => (args as string[])[0] === "commit")).toBe(false);
  expect(state.command.mock.calls.some(([program, args]) => program === "cargo" && (args as string[])[0] === "test")).toBe(true);
  expect(state.tagCommit).toBe(releaseSha);
  expect(state.command).toHaveBeenCalledWith("gh", expect.arrayContaining([`target_commitish=${originalSha}`]), true);
});

it.each([true, false])("reuses only a tag whose validation belongs to the same release commit: %s", async valid => {
  writeFileSync(path.join(state.root, "package.json"), JSON.stringify({ version: "0.8.4-beta" }));
  state.head = releaseSha; state.remote = releaseSha; state.tagCommit = releaseSha;
  state.tagContents = `Jarvis 0.8.4-beta\n\nNotas preservadas.\n\nJarvis-Local-Checks-v1: ${valid ? releaseSha : originalSha}\n`;
  await import("./release");
  expect(process.exitCode).toBe(valid ? 0 : 1);
  expect(state.command.mock.calls.some(([program]) => ["bun", "cargo"].includes(String(program)))).toBe(false);
  expect(state.command.mock.calls.some(([program, args]) => program === "gh" && (args as string[])[0] === "api")).toBe(false);
  expect(state.command.mock.calls.some(([, args]) => (args as string[])[0] === "push")).toBe(valid);
});
it("retries a validated legacy tag with empty notes without regenerating or replacing it", async () => {
  writeFileSync(path.join(state.root, "package.json"), JSON.stringify({ version: "0.8.4-beta" }));
  state.head = releaseSha; state.remote = releaseSha; state.tagCommit = releaseSha;
  const contents = `Jarvis 0.8.4-beta\n\n\n\nJarvis-Local-Checks-v1: ${releaseSha}\n`;
  state.tagContents = contents;
  await import("./release");
  expect(process.exitCode).toBe(0);
  expect(state.tagContents).toBe(contents);
  expect(state.command.mock.calls.some(([program, args]) => program === "gh" && (args as string[])[0] === "api")).toBe(false);
  expect(state.command.mock.calls.some(([, args]) => (args as string[])[0] === "tag" && (args as string[]).includes("--file"))).toBe(false);
});
