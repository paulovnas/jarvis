// Jarvis owns lifecycle and scope. Never invoke Graft's CLI, MCP bootstrap,
// host hooks, telemetry, enrichment or cross-workspace federation.
import { lstatSync, mkdirSync, readdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { performance } from "node:perf_hooks";
import { buildGraph } from "./node_modules/@nanonets/graft/dist/graph/build.js";
import { probeDrift, isClean } from "./node_modules/@nanonets/graft/dist/graph/fingerprint.js";
import { readGraph, wiringPath, writeGraph } from "./node_modules/@nanonets/graft/dist/graph/write.js";
import { askIndexPath, counts, readAskIndex, tokenize } from "./node_modules/@nanonets/graft/dist/ask/index-file.js";
import { ask, skeleton } from "./node_modules/@nanonets/graft/dist/ask/ask.js";
import { buildRepoMap } from "./node_modules/@nanonets/graft/dist/graph/map.js";
import { resolveSymbol, edgeWalk } from "./node_modules/@nanonets/graft/dist/graph/traverse.js";
import { grepGraph } from "./node_modules/@nanonets/graft/dist/search/grep.js";
import { readSourceFile } from "./node_modules/@nanonets/graft/dist/util/source.js";

const names = new Set(["graft_repo_map", "graft_find_code", "graft_file_api", "graft_trace_calls", "graft_find_all", "graft_check_freshness"]);
function confined(root, path) {
  if (typeof path !== "string" || !path || path.includes("\\") || path.includes(":") || isAbsolute(path) || path.split("/").includes("..")) throw new Error("Project-relative path required");
  const file = resolve(root, path);
  const rel = relative(root, realpathSync(file));
  if (!rel || rel.startsWith(`..${sep}`) || rel === ".." || isAbsolute(rel) || lstatSync(file).isSymbolicLink()) throw new Error("Path escapes project or is a symlink");
  return file;
}
function guardCache(cache) {
  for (const entry of readdirSync(cache, { withFileTypes: true })) {
    const file = join(cache, entry.name);
    if (entry.isSymbolicLink()) throw new Error("Invalid private cache link");
    if (entry.isDirectory()) guardCache(file);
  }
}
function omitSavings(value) {
  if (Array.isArray(value)) return value.map(omitSavings);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).filter(([key]) => key !== "saved" && key !== "rules" && key !== "ranking").map(([key, value]) => [key, omitSavings(value)]));
  return value;
}
function excerpt(root, pointer, full) {
  const span = /^(.*):L(\d+)-L(\d+)$/.exec(pointer);
  if (!span) return undefined;
  const file = confined(root, span[1]);
  const source = readSourceFile(file);
  if (source === null) return undefined;
  const lines = source.split("\n");
  const start = Number(span[2]);
  const end = Math.min(Number(span[3]), start + (full ? 79 : 7));
  const text = lines.slice(start - 1, end).join("\n");
  const max = full ? 12000 : 2500;
  return { path: span[1], startLine: start, endLine: end, text: text.slice(0, max), truncated: end < Number(span[3]) || text.length > max };
}
function bounded(value) {
  // Large results still pass through Context-mode once. Cap the adapter's IPC
  // envelope as well, preserving valid JSON and saying when detail was dropped.
  const text = JSON.stringify(value);
  if (Buffer.byteLength(text) <= 48000) return value;
  return { preview: text.slice(0, 12000), truncated: true, note: "Large structural result. Narrow the query with in/limit/depth or read an exact source range; this preview is incomplete." };
}
function repositoryPaths(paths = []) {
  if (!Array.isArray(paths) || paths.some(path => typeof path !== "string" || !path || path.includes("\\") || path.includes(":") || isAbsolute(path) || path.split("/").includes(".."))) throw new Error("Invalid configured repository path");
  return [...new Set((paths.length ? paths : ["."]).map(path => path.split("/").filter(part => part && part !== ".").join("/") || "."))].sort();
}
async function projectGraph(root, contextDir, paths, checkOnly) {
  const topology = JSON.stringify(paths);
  const topologyPath = join(contextDir, "jarvis-repositories.json");
  let previous;
  try { previous = JSON.parse(readFileSync(topologyPath, "utf8")); } catch { /* First project query. */ }
  const sameTopology = JSON.stringify(previous?.paths) === topology;
  const repositories = [];
  const combined = readGraph(wiringPath(contextDir));
  const searchIndex = readAskIndex(contextDir);
  let rebuilt = !sameTopology || !combined || !searchIndex;
  for (const path of paths) {
    const repo = { path };
    repositories.push(repo);
    try {
      repo.root = path === "." ? root : confined(root, path);
      if (!lstatSync(repo.root).isDirectory()) throw new Error("Configured repository is not a directory");
      repo.cache = join(contextDir, `repo-${createHash("sha256").update(path).digest("hex")}`);
      if (!checkOnly) mkdirSync(repo.cache, { recursive: true });
      repo.graph = readGraph(wiringPath(repo.cache));
      repo.drift = repo.graph ? probeDrift(repo.root, repo.cache) : null;
      if (!checkOnly && (!repo.graph || !repo.drift || !isClean(repo.drift))) {
        repo.build = await buildGraph(repo.root, { contextDir: repo.cache, graphOnly: true, lsp: false });
        writeFileSync(join(repo.cache, "jarvis-coverage.json"), JSON.stringify({ files: repo.build.files, languages: repo.build.languages, errors: repo.build.errors }));
        repo.graph = readGraph(wiringPath(repo.cache));
        repo.drift = probeDrift(repo.root, repo.cache);
        rebuilt = true;
      }
      if (!repo.graph || !Array.isArray(repo.graph.nodes) || !Array.isArray(repo.graph.edges) || !repo.drift || !isClean(repo.drift)) {
        if (checkOnly) continue;
        throw new Error("Graph is not current; use focused native discovery and retry after edits settle");
      }
      // Validate every child graph against its own root before merging scopes.
      for (const file of new Set(repo.graph.nodes.map(node => node.path))) confined(repo.root, file);
      try { repo.coverage = JSON.parse(readFileSync(join(repo.cache, "jarvis-coverage.json"), "utf8")); }
      catch { repo.coverage = { errors: ["Coverage metadata unavailable"] }; }
      repo.index = readAskIndex(repo.cache);
    } catch (cause) {
      repo.error = String(cause?.message ?? cause).slice(0, 300);
      rebuilt = true;
    }
  }
  const healthy = repositories.filter(repo => !repo.error && repo.graph && repo.drift && isClean(repo.drift));
  // Child refresh can finish before cancellation interrupts the project merge.
  // Bind the aggregate to its actual inputs, including returning repositories.
  const inputs = createHash("sha256").update(JSON.stringify(healthy.map(repo => [repo.path, repo.graph, repo.index]))).digest("hex");
  rebuilt ||= previous?.inputs !== inputs;
  const freshness = {
    current: sameTopology && previous?.inputs === inputs && !!combined && !!searchIndex && healthy.length === paths.length,
    missing: repositories.some(repo => !repo.graph), rebuilt: false,
    repositories: repositories.map(repo => ({ path: repo.path, current: !repo.error && !!repo.graph && !!repo.drift && isClean(repo.drift), missing: !repo.graph, error: repo.error, drift: repo.drift && Object.fromEntries(Object.entries(repo.drift).map(([kind, files]) => [kind, { total: files.length, paths: files.slice(0, 30) }])) })),
  };
  if (checkOnly) return { freshness };
  if (!healthy.length) throw new Error(repositories.map(repo => `${repo.path}: ${repo.error ?? "Graph unavailable"}`).join("; "));

  const prefix = (repo, value) => repo.path === "." ? value : `${repo.path}/${value}`;
  const owner = file => paths.filter(path => path === "." || file.startsWith(`${path}/`)).sort((a, b) => b.length - a.length)[0];
  const nodes = [], edges = [], docs = [], scopes = [];
  for (const repo of healthy) {
    const included = new Set(repo.graph.nodes.filter(node => owner(prefix(repo, node.path)) === repo.path).map(node => node.id));
    nodes.push(...repo.graph.nodes.filter(node => included.has(node.id)).map(node => ({ ...node, id: prefix(repo, node.id), path: prefix(repo, node.path) })));
    edges.push(...repo.graph.edges.filter(edge => included.has(edge.source)).map(edge => ({ ...edge, source: prefix(repo, edge.source), target: included.has(edge.target) ? prefix(repo, edge.target) : edge.target })));
    for (const scope of repo.graph.meta.scopes ?? [{ prefix: "", label: "", markers: [] }]) {
      const path = repo.path === "." ? scope.prefix : `${repo.path}${scope.prefix ? `/${scope.prefix}` : ""}`;
      scopes.push({ ...scope, prefix: path, label: path });
    }
    const index = repo.index;
    if (!index) repo.coverage.errors = [...(repo.coverage.errors ?? []), "Search metadata unavailable"];
    const byId = new Map(repo.graph.nodes.map(node => [node.id, node]));
    for (const doc of index?.docs ?? []) if (included.has(doc.id)) docs.push({ ...doc, id: prefix(repo, doc.id), path: [...counts(tokenize(prefix(repo, byId.get(doc.id).path)))] });
  }
  const languages = [...new Set(healthy.flatMap(repo => repo.coverage.languages ?? []))].sort();
  const graph = { meta: { version: 1, nodeCount: nodes.length, edgeCount: edges.length, languages, scopes: scopes.sort((a, b) => b.prefix.length - a.prefix.length) }, nodes, edges };
  if (rebuilt) {
    // A canceled partial publication must not retain an older complete receipt.
    writeFileSync(topologyPath, "null");
    writeGraph(graph, contextDir);
    // Preserve each repository's body tokens while ranking in one project corpus.
    const df = new Map();
    for (const doc of docs) for (const term of new Set([...doc.name, ...doc.path, ...doc.body].map(([term]) => term))) df.set(term, (df.get(term) ?? 0) + 1);
    mkdirSync(join(contextDir, ".cache"), { recursive: true });
    writeFileSync(askIndexPath(contextDir), JSON.stringify({ version: 1, docs, docCount: docs.length, df: [...df], avgBodyLen: docs.length ? docs.reduce((sum, doc) => sum + doc.body.reduce((sum, [, count]) => sum + count, 0), 0) / docs.length : 0 }));
    writeFileSync(topologyPath, JSON.stringify({ paths, inputs }));
  }
  const errors = repositories.flatMap(repo => repo.error ? [`${repo.path}: ${repo.error}`] : (repo.coverage.errors ?? []).map(error => `${repo.path}: ${String(error).slice(0, 300)}`));
  const coverage = { files: new Set(nodes.map(node => node.path)).size, languages, partial: errors.length > 0, errors: errors.slice(0, 8), repositories: repositories.map(repo => ({ path: repo.path, files: repo.coverage?.files, error: repo.error })), note: "All configured repositories are checked before retrieval. Only supported, nonignored source is indexed. AST edges do not prove dynamic dispatch coverage or cross-repository runtime calls." };
  return { graph, repositories: healthy, freshness: { ...freshness, current: healthy.length === paths.length, rebuilt, parsed: healthy.reduce((sum, repo) => sum + (repo.build?.parsed ?? 0), 0), reused: healthy.reduce((sum, repo) => sum + (repo.build?.reused ?? (repo.build ? 0 : repo.coverage.files ?? 0)), 0) }, coverage };
}
async function execute(request) {
  const start = performance.now();
  const { tool, args = {} } = request;
  if (!names.has(tool) || !args || typeof args !== "object" || Array.isArray(args)) throw new Error("Unknown structural tool or invalid arguments");
  const root = realpathSync(request.root);
  const contextDir = realpathSync(request.contextDir);
  const cacheRelative = relative(root, contextDir);
  if (!lstatSync(root).isDirectory() || !lstatSync(contextDir).isDirectory() || contextDir === root || !(cacheRelative === ".." || cacheRelative.startsWith(`..${sep}`) || isAbsolute(cacheRelative))) throw new Error("Graph cache must be outside the project");
  guardCache(contextDir);
  const paths = repositoryPaths(request.repositories);
  const { graph, repositories, freshness, coverage } = await projectGraph(root, contextDir, paths, tool === "graft_check_freshness");
  if (tool === "graft_check_freshness") return { ok: true, freshness, result: { note: "Drift is reported for every configured repository before refresh. The next structural retrieval refreshes automatically." }, durationMs: performance.now() - start };
  // Reject corrupted/redirected private graphs before any upstream source reader.
  for (const path of new Set(graph.nodes.map(node => node.path))) confined(root, path);
  let result;
  switch (tool) {
    case "graft_repo_map": result = buildRepoMap(graph, { maxDirs: args.max_dirs ?? 16 }); break;
    case "graft_find_code": {
      const found = ask(root, args.query, { contextDir, source: false, limit: args.limit ?? 5, in: args.in });
      result = { mode: found.mode, note: found.note, hits: found.hits.map(hit => ({ ...hit, excerpt: excerpt(root, hit.pointer, args.full === true) })) };
      break;
    }
    case "graft_file_api": result = skeleton(root, args.file, { contextDir }); break;
    case "graft_trace_calls": {
      const symbols = resolveSymbol(graph, args.symbol, args.in ? { in: args.in } : {});
      result = { matches: symbols.map(symbol => ({ symbol: { name: symbol.name, path: symbol.path, span: symbol.span, kind: symbol.kind }, hits: edgeWalk(graph, symbol, args.direction ?? "in", args.depth === "all" ? Infinity : (args.depth ?? 1)).map(hit => ({ relation: hit.relation, depth: hit.depth, name: hit.node?.name, path: hit.node?.path, span: hit.node?.span, unresolved: !hit.node })) })), note: symbols.length ? "An empty edge list means no indexed edges, not proof that no callers exist. Check dynamic usage with native search/tests." : "No indexed match. Check spelling, narrow the path or use focused native search for unindexed code." };
      break;
    }
    case "graft_find_all": result = grepGraph(graph, root, args.pattern, { in: args.in, fixed: args.fixed, ignoreCase: args.ignore_case, maxHits: 150 }); break;
  }
  // A concurrent edit during retrieval must never produce authoritative old spans.
  for (const repo of repositories) {
    const after = probeDrift(repo.root, repo.cache);
    if (!after || !isClean(after)) throw new Error("Source changed during retrieval; discard structural spans and use current native source");
  }
  return { ok: true, freshness, coverage, result: bounded(omitSavings(result)), durationMs: Math.round(performance.now() - start) };
}

let input = "";
try {
  for await (const chunk of process.stdin) { input += chunk; if (input.length > 16000) throw new Error("Request too large"); }
  const output = await execute(JSON.parse(input));
  if (Buffer.byteLength(JSON.stringify(output)) > 60000) {
    output.freshness = { current: output.freshness.current, missing: output.freshness.missing, rebuilt: output.freshness.rebuilt };
    output.coverage = { partial: output.coverage?.partial, note: "Coverage detail omitted to fit IPC budget" };
    output.result = bounded(output.result);
    if (Buffer.byteLength(JSON.stringify(output)) > 60000) output.result = { truncated: true, note: "Result exceeded IPC budget. Narrow your structural query." };
  }
  process.stdout.write(JSON.stringify(output));
} catch (cause) {
  process.stdout.write(JSON.stringify({ ok: false, error: String(cause?.message ?? cause).slice(0, 500) }));
}
