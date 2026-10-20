import { readFile, stat, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { runPixelCockpitVerification, type PixelRuntime } from "./runtime-smoke";
import { inspectArtifact } from "./verify-artifacts";

type Data = Record<string, unknown>;
const record = (value: unknown): Data => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Data : {};

const args = process.argv.slice(2);
const connectionPath = args.find((value) => !value.startsWith("--"));
if (!connectionPath) throw new Error("Usage: bun scripts/pixel-cockpit/run-native-probe.ts <connection.json> [--generate] [--source=ID --animation=ID --effect=ID]");
const connection = record(JSON.parse(await readFile(resolve(connectionPath), "utf8")));
const url = new URL(String(connection.url));
if (url.protocol !== "http:" || url.hostname !== "127.0.0.1") throw new Error("The native probe must be on the local loopback interface.");
const token = String(connection.token);
const output = String(connection.output);
const request = async (path: string, body?: Data) => {
  const response = await fetch(new URL(path, url), {
    method: body ? "POST" : "GET",
    headers: { "x-jarvis-probe": token, ...(body ? { "content-type": "application/json" } : {}) },
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(120_000),
  });
  const value: unknown = await response.json();
  if (!response.ok) throw new Error(String(record(value).error ?? `Native probe HTTP ${response.status}`));
  return record(value);
};
const tool = (name: string, arguments_: Data) => request("/tool", { name, arguments: arguments_ });
const receipts = new Map<string, Data>();
const ids = new Map<string, string>();
const runtime: PixelRuntime = {
  async start(command) {
    const isCatalog = command.action === "catalog";
    const receipt = await tool(isCatalog ? "pixel_art_docs" : "pixel_art_run", isCatalog ? command.args ?? {} : {
      action: command.action, arguments: command.args ?? {}, yieldTimeMs: 1000,
    });
    receipts.set(command.id, isCatalog ? { id: command.id, action: "catalog", status: "completed", result: receipt } : receipt);
    if (!isCatalog) {
      if (typeof receipt.jobId !== "string") throw new Error("The native tool did not return a job ID.");
      ids.set(command.id, receipt.jobId);
    }
    return receipt;
  },
  async snapshot(id) {
    if (!id) return request("/info");
    const nativeId = ids.get(id);
    if (!nativeId) return receipts.get(id);
    const receipt = await tool("pixel_art_status", { jobId: nativeId, yieldTimeMs: 0 });
    receipts.set(id, receipt);
    return receipt;
  },
};
const option = (name: string) => args.find((value) => value.startsWith(`--${name}=`))?.slice(name.length + 3);
let lastPhase = "";
const report = await runPixelCockpitVerification(runtime, {
  runRealGeneration: args.includes("--generate"),
  sourceId: option("source"), animationId: option("animation"), effectId: option("effect"),
  onProgress(receipt) {
    const phase = `${String(receipt.action)}: ${String(receipt.status)} ${String(receipt.phase ?? "")}`;
    if (phase !== lastPhase) { process.stdout.write(`${phase}\n`); lastPhase = phase; }
  },
});
const paths = new Set<string>();
for (const receipt of receipts.values()) {
  for (const artifact of Array.isArray(receipt.artifacts) ? receipt.artifacts : []) {
    if (typeof record(artifact).path === "string") paths.add(String(record(artifact).path));
  }
}
const artifacts: Data[] = [];
for (const path of paths) {
  try {
    if (/\.(?:png|apng|gif|webp|zip|json)$/i.test(path)) {
      artifacts.push(record(await inspectArtifact(path)));
    } else {
      const metadata = await stat(path);
      if (!metadata.isFile() || metadata.size > 128 * 1024 * 1024) throw new Error("Expected an artifact file up to 128 MiB.");
      const bytes = await readFile(path);
      artifacts.push({ path, bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex"), format: "other", structureChecked: false });
    }
  }
  catch (error) { artifacts.push({ path, error: error instanceof Error ? error.message : String(error) }); }
}
const invalid = artifacts.filter((artifact) => typeof artifact.error === "string");
if (invalid.length && report.status === "passed") {
  report.status = "blocked";
  report.error = `${invalid.length} output artifact(s) failed structural verification. Inspect artifact-inspection.json.`;
}
await writeFile(resolve(output, "artifact-inspection.json"), `${JSON.stringify(artifacts, null, 2)}\n`);
await request("/finish", {
  ...report,
  evidenceScope: "Native pixel-art tools and original CEF suite with actual Codex generation when --generate is present. This does not prove LLM agent selection or visual acceptance.",
  inspectedArtifacts: artifacts.length,
});
process.stdout.write(`Native report: ${resolve(output, "native-tool-report.json")}\n`);
if (report.status !== "passed") {
  process.stderr.write(`${report.error ?? "The real runtime verification did not pass."}\n`);
  process.exitCode = 1;
}
