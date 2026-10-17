import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, renameSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { afterEach, expect, it } from "vitest";

type Result = { ok: boolean; error?: string; freshness: { current: boolean; rebuilt: boolean; missing?: boolean }; coverage: { files: number; partial: boolean; repositories: { path: string; files?: number; error?: string }[] }; result: { hits?: { pointer: string; excerpt?: { path: string } }[]; file?: string; entries?: { name: string }[]; totals?: { files: number } } };
const temporary: string[] = [];
afterEach(() => temporary.splice(0).forEach(dir => rmSync(dir, { recursive: true, force: true })));

function fixture() {
  const directory = mkdtempSync(path.join(tmpdir(), "jarvis-graft-")); temporary.push(directory);
  const root = path.join(directory, "project"); const cache = path.join(directory, "cache");
  mkdirSync(root); mkdirSync(cache);
  for (const repo of [".", "frontend", "backend", "unregistered"]) {
    const location = path.join(root, repo); mkdirSync(path.join(location, "src"), { recursive: true });
    execFileSync("git", ["init", "--quiet", location]);
    writeFileSync(path.join(location, "src", "math.ts"), `export function ${repo === "." ? "parent" : repo}Value() { return 1; }\n`);
  }
  const installed = process.env.JARVIS_GRAFT_TEST_PACKAGE;
  let adapter = readFileSync("src-tauri/src/core/graft/adapter.mjs", "utf8");
  if (installed) symlinkSync(path.join(installed, "node_modules"), path.join(directory, "node_modules"), process.platform === "win32" ? "junction" : "dir");
  else adapter = adapter.replace(/"\.\/node_modules\/@nanonets\/graft\/[^"\n]+"/g, '"./stub.mjs"');
  writeFileSync(path.join(directory, "adapter.mjs"), adapter);
  writeFileSync(path.join(directory, "stub.mjs"), readFileSync("scripts/fixtures/graft-runtime.mjs", "utf8"));
  const query = (tool: string, args: Record<string, unknown> = {}, repositories = ["frontend", "backend"]): Result => JSON.parse(execFileSync(process.execPath, [path.join(directory, "adapter.mjs")], { input: JSON.stringify({ root, contextDir: cache, tool, args, repositories }), encoding: "utf8", env: { ...process.env, GRAFT_REFRESH: "hash" } })) as Result;
  const refreshChild = (repo: string) => execFileSync(process.execPath, ["--input-type=module", "-e", "await (await import(process.argv[1])).buildGraph(process.argv[2], { contextDir: process.argv[3], graphOnly: true, lsp: false })", pathToFileURL(installed ? path.join(installed, "node_modules/@nanonets/graft/dist/graph/build.js") : path.join(directory, "stub.mjs")).href, path.join(root, repo), path.join(cache, `repo-${createHash("sha256").update(repo).digest("hex")}`)], { env: { ...process.env, GRAFT_NO_GITIGNORE: "1", GRAFT_NO_IGNORE: "1", GRAFT_NO_SEED: "1" } });
  const interruptPublication = () => {
    const interrupted = adapter.replace("writeFileSync(topologyPath, JSON.stringify({ paths, inputs }));", 'throw new Error("Interrupted aggregate publication");');
    expect(interrupted).not.toBe(adapter);
    writeFileSync(path.join(directory, "adapter.mjs"), interrupted);
    return () => writeFileSync(path.join(directory, "adapter.mjs"), adapter);
  };
  return { root, cache, query, refreshChild, interruptPublication };
}

it("indexes every registered nested repository with project-relative lookup paths", () => {
  const { query } = fixture();
  for (const repo of ["frontend", "backend"]) {
    const result = query("graft_find_code", { query: `${repo}Value`, in: `${repo}/src` });
    expect(result.ok, result.error).toBe(true);
    expect(result.result.hits?.[0]).toMatchObject({ pointer: `${repo}/src/math.ts:L1-L1`, excerpt: { path: `${repo}/src/math.ts` } });
    expect(result.coverage).toMatchObject({ files: 2, partial: false });
  }
  expect(query("graft_find_code", { query: "unregisteredValue" }).result.hits?.every(hit => !hit.pointer.startsWith("unregistered/"))).toBe(true);
}, 30_000);

it("detects content edits with unchanged size and mtime, and additions, renames and deletions in each repository", async () => {
  const { root, query } = fixture();
  query("graft_repo_map");
  expect(query("graft_check_freshness").freshness.current).toBe(true);
  const { statSync, utimesSync, unlinkSync } = await import("node:fs");
  for (const repo of ["frontend", "backend"]) {
    const file = path.join(root, repo, "src/math.ts"); const before = statSync(file);
    writeFileSync(file, `export function ${repo}Other() { return 2; }\n`); utimesSync(file, before.atime, before.mtime);
    expect(query("graft_check_freshness").freshness.current).toBe(false);
    const api = query("graft_file_api", { file: `${repo}/src/math.ts` });
    expect(api.result.entries?.map(entry => entry.name)).toContain(`${repo}Other`);
    expect(api.freshness.rebuilt).toBe(true);
    renameSync(file, path.join(root, repo, "src/renamed.ts"));
    expect(query("graft_check_freshness").freshness.current).toBe(false);
    expect(query("graft_file_api", { file: `${repo}/src/renamed.ts` }).result.file).toBe(`${repo}/src/renamed.ts`);
    writeFileSync(path.join(root, repo, "src/new.ts"), "export function newlyAdded() {}\n");
    expect(query("graft_repo_map").result.totals?.files).toBe(3);
    unlinkSync(path.join(root, repo, "src/new.ts"));
    expect(query("graft_repo_map").result.totals?.files).toBe(2);
  }
}, 30_000);

it("invalidates combined coverage when registered repositories are added or removed", () => {
  const { query } = fixture();
  query("graft_repo_map", {}, ["frontend"]);
  expect(query("graft_check_freshness", {}, ["frontend", "backend"]).freshness.current).toBe(false);
  expect(query("graft_repo_map").result.totals?.files).toBe(2);
  expect(query("graft_find_code", { query: "backendValue" }, ["frontend"]).result.hits?.every(hit => !hit.pointer.startsWith("backend/"))).toBe(true);
  expect(query("graft_repo_map", {}, ["frontend", "frontend"]).result.totals?.files).toBe(1);
});

it("reports a missing registered repository as partial coverage while retaining healthy results", () => {
  const { query } = fixture();
  const result = query("graft_find_code", { query: "frontendValue" }, ["frontend", "missing"]);
  expect(result.ok, result.error).toBe(true);
  expect(result.coverage.partial).toBe(true);
  expect(result.coverage.repositories.find(repo => repo.path === "missing")?.error).toBeTruthy();
  expect(result.result.hits?.[0].pointer).toContain("frontend/src/math.ts:");
});

it("restores unchanged repository results after a temporary directory outage", () => {
  const { root, query } = fixture();
  expect(query("graft_repo_map").result.totals?.files).toBe(2);
  renameSync(path.join(root, "backend"), path.join(root, "offline-backend"));
  expect(query("graft_repo_map").coverage.partial).toBe(true);
  renameSync(path.join(root, "offline-backend"), path.join(root, "backend"));
  expect(query("graft_check_freshness").freshness.current).toBe(false);
  const found = query("graft_find_code", { query: "backendValue", in: "backend/src" });
  expect(found.ok, found.error).toBe(true);
  expect(found.result.hits?.[0].pointer).toContain("backend/src/math.ts:");
  expect(found.coverage.partial).toBe(false);
}, 30_000);

it("recovers a child refresh interrupted before publishing the combined index", () => {
  const { root, query, refreshChild } = fixture();
  query("graft_repo_map");
  writeFileSync(path.join(root, "backend/src/math.ts"), "export function backendChanged() {}\n");
  // A killed query may leave current child artifacts and an older aggregate.
  refreshChild("backend");
  expect(query("graft_check_freshness").freshness.current).toBe(false);
  const api = query("graft_file_api", { file: "backend/src/math.ts" });
  expect(api.result.entries?.map(entry => entry.name)).toContain("backendChanged");
  expect(api.freshness.rebuilt).toBe(true);
}, 30_000);

it("recovers an interrupted partial publication after an unchanged repository returns", () => {
  const { root, query, interruptPublication } = fixture();
  query("graft_repo_map");
  renameSync(path.join(root, "backend"), path.join(root, "offline-backend"));
  const resume = interruptPublication();
  expect(query("graft_repo_map").ok).toBe(false);
  resume();
  renameSync(path.join(root, "offline-backend"), path.join(root, "backend"));
  const found = query("graft_find_code", { query: "backendValue", in: "backend/src" });
  expect(found.ok, found.error).toBe(true);
  expect(found.result.hits?.[0].pointer).toContain("backend/src/math.ts:");
  expect(found.freshness.rebuilt).toBe(true);
}, 30_000);

it("retains the default root and rejects repository paths that escape it", () => {
  const { root, query } = fixture();
  expect(query("graft_find_code", { query: "parentValue" }, []).result.hits?.[0].pointer).toContain("src/math.ts:");
  for (const repository of ["../outside", "/outside", "C:\\outside"]) expect(query("graft_repo_map", {}, [repository]).ok).toBe(false);
  if (process.platform !== "win32") {
    symlinkSync(tmpdir(), path.join(root, "redirected"));
    expect(query("graft_repo_map", {}, ["redirected"]).ok).toBe(false);
  }
});
