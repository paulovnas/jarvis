// Jarvis owns lifecycle and scope. Never invoke Graft's CLI, MCP bootstrap,
// host hooks, telemetry, enrichment or cross-workspace federation.
import { lstatSync, readdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { performance } from "node:perf_hooks";
import { buildGraph } from "./node_modules/@nanonets/graft/dist/graph/build.js";
import { probeDrift, isClean } from "./node_modules/@nanonets/graft/dist/graph/fingerprint.js";
import { readGraph, wiringPath } from "./node_modules/@nanonets/graft/dist/graph/write.js";
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
async function execute(request) {
  const start = performance.now();
  const { tool, args = {} } = request;
  if (!names.has(tool) || !args || typeof args !== "object" || Array.isArray(args)) throw new Error("Unknown structural tool or invalid arguments");
  const root = realpathSync(request.root);
  const contextDir = realpathSync(request.contextDir);
  const cacheRelative = relative(root, contextDir);
  if (!lstatSync(root).isDirectory() || !lstatSync(contextDir).isDirectory() || contextDir === root || !(cacheRelative === ".." || cacheRelative.startsWith(`..${sep}`) || isAbsolute(cacheRelative))) throw new Error("Graph cache must be outside the project");
  guardCache(contextDir);
  let graph = readGraph(wiringPath(contextDir));
  let drift = graph ? probeDrift(root, contextDir) : null;
  if (tool === "graft_check_freshness") return { ok: true, freshness: { current: !!graph && !!drift && isClean(drift), missing: !graph, drift: drift && Object.fromEntries(Object.entries(drift).map(([kind,paths]) => [kind, { total: paths.length, paths: paths.slice(0, 30) }])), rebuilt: false }, result: { note: "Drift is reported before refresh. The next structural retrieval refreshes automatically." }, durationMs: performance.now() - start };
  let build;
  if (!graph || !drift || !isClean(drift)) {
    build = await buildGraph(root, { contextDir, graphOnly: true, lsp: false });
    writeFileSync(join(contextDir, "jarvis-coverage.json"), JSON.stringify({ files: build.files, parsed: build.parsed, reused: build.reused, languages: build.languages, errors: build.errors }));
    graph = readGraph(wiringPath(contextDir));
    drift = probeDrift(root, contextDir);
  }
  if (!graph || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges) || !drift || !isClean(drift)) throw new Error("Graph is not current; use focused native discovery and retry a read-only query after edits settle");
  // Reject corrupted/redirected private graphs before any upstream source reader.
  for (const path of new Set(graph.nodes.map(node => node.path))) confined(root, path);
  let coverage;
  try { coverage = JSON.parse(readFileSync(join(contextDir, "jarvis-coverage.json"), "utf8")); } catch { coverage = { errors: ["Coverage metadata unavailable"] }; }
  coverage = { files: coverage.files, languages: coverage.languages, partial: coverage.errors?.length !== 0, errors: coverage.errors?.slice(0, 8).map(error => String(error).slice(0, 300)), note: "Only supported code visible to Graft is indexed; generated/dependency/ignored/hidden paths and unrelated nested repositories may be absent. AST edges do not prove dynamic dispatch coverage." };
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
  const after = probeDrift(root, contextDir);
  if (!after || !isClean(after)) throw new Error("Source changed during retrieval; discard structural spans and use current native source");
  return { ok: true, freshness: { current: true, rebuilt: !!build, parsed: build?.parsed ?? 0, reused: build?.reused ?? coverage.files }, coverage, result: bounded(omitSavings(result)), durationMs: Math.round(performance.now() - start) };
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
