// Offline stand-in for Graft's public graph/query contracts. Real parser
// compatibility is verified by the native Core installation health check.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export const wiringPath = dir => join(dir, ".graph", "wiring.json");
export function readGraph(path) {
  try { return JSON.parse(readFileSync(path, "utf8")); } catch { return null; }
}
export function writeGraph(graph, dir) {
  mkdirSync(dirname(wiringPath(dir)), { recursive: true });
  writeFileSync(wiringPath(dir), JSON.stringify(graph));
}
function sources(root) {
  return execFileSync("git", ["-C", root, "ls-files", "--cached", "--others", "--exclude-standard", "-z"], { encoding: "utf8" })
    .split("\0").filter(file => /\.ts$/.test(file)).map(file => ({
      file, text: readFileSync(join(root, file), "utf8"),
    }));
}
function hashes(root) {
  return Object.fromEntries(sources(root).map(({ file, text }) => [file, createHash("sha256").update(text).digest("hex")]));
}
export function probeDrift(root, dir) {
  const prior = readGraph(join(dir, "fingerprint.json"));
  if (!prior) return null;
  const now = hashes(root);
  return { added: Object.keys(now).filter(p => !(p in prior)), removed: Object.keys(prior).filter(p => !(p in now)), changed: Object.keys(now).filter(p => p in prior && now[p] !== prior[p]) };
}
export const isClean = drift => Object.values(drift).every(paths => paths.length === 0);
export async function buildGraph(root, { contextDir }) {
  const files = sources(root);
  const nodes = files.flatMap(({ file, text }) => [
    { id: `${file}#file`, path: file, kind: "file", name: file, span: "L1-L1" },
    ...[...text.matchAll(/function (\w+)/g)].map(match => ({ id: `${file}#${match[1]}`, path: file, kind: "function", name: match[1], span: "L1-L1", body_text: text })),
  ]);
  writeGraph({ meta: { version: 1, nodeCount: nodes.length, edgeCount: 0, languages: ["typescript"] }, nodes, edges: [] }, contextDir);
  writeFileSync(join(contextDir, "fingerprint.json"), JSON.stringify(hashes(root)));
  writeAskIndex(contextDir, { nodes });
  return { files: files.length, parsed: files.length, reused: 0, errors: [], languages: files.length ? ["typescript"] : [] };
}
export const tokenize = value => value.toLowerCase().split(/\W+/).filter(Boolean);
export const counts = tokens => new Map(tokens.map(token => [token, 1]));
export const askIndexPath = dir => join(dir, ".cache", "ask-index.json");
export const readAskIndex = dir => readGraph(askIndexPath(dir));
export function writeAskIndex(dir, graph) {
  mkdirSync(dirname(askIndexPath(dir)), { recursive: true });
  writeFileSync(askIndexPath(dir), JSON.stringify({ version: 1, docs: graph.nodes.map(node => ({ id: node.id, name: [...counts(tokenize(node.name))], path: [...counts(tokenize(node.path))], body: [...counts(tokenize(node.body_text ?? ""))] })) }));
}
export function ask(root, query, { contextDir, limit = 5, in: prefix }) {
  const graph = readGraph(wiringPath(contextDir));
  if (prefix && !graph.nodes.some(node => node.path.startsWith(prefix))) throw new Error(`nothing indexed under "${prefix}/"`);
  const hits = graph.nodes.filter(node => node.name.includes(query) && (!prefix || node.path.startsWith(prefix))).slice(0, limit).map(node => ({ pointer: `${node.path}:${node.span}`, title: node.name }));
  return { mode: hits.length ? "lexical" : "empty", hits };
}
export function skeleton(root, file, { contextDir }) {
  const graph = readGraph(wiringPath(contextDir));
  const paths = [...new Set(graph.nodes.filter(node => node.path === file || node.path.endsWith(`/${file}`)).map(node => node.path))];
  if (paths.length > 1) throw new Error("Ambiguous basename");
  return { file: paths[0] ?? file, entries: graph.nodes.filter(node => node.path === paths[0] && node.kind !== "file") };
}
export function buildRepoMap(graph) { return { totals: { files: graph.nodes.filter(n => n.kind === "file").length }, dirs: graph.nodes.filter(n => n.kind === "file").map(n => ({ path: n.path })) }; }
export function resolveSymbol(graph, symbol) { return graph.nodes.filter(node => node.name === symbol || node.path === symbol); }
export function edgeWalk() { return []; }
export function grepGraph(graph, root, pattern) { return { totalHits: graph.nodes.filter(n => n.kind === "file" && readFileSync(join(root, n.path), "utf8").includes(pattern)).length }; }
export const readSourceFile = path => existsSync(path) ? readFileSync(path, "utf8") : null;
