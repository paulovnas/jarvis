import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { exposeSuiteController, extractController, exportedModuleCatalog, forwardPublicOperationErrors, typeDeclarationCatalog } from "./pixel-cockpit/adopt";
import { publicAppMethods } from "./pixel-cockpit/contract";
import { adoptTheme } from "./pixel-cockpit/theme";
import { adoptUi, presentationRuntimeSource } from "./pixel-cockpit/ui-adapter";
import { localizePortugueseCopy } from "./pixel-cockpit/portuguese-copy";
import { adoptManagedHealth } from "./pixel-cockpit/managed-health";
import { adoptManagedRunner } from "./pixel-cockpit/managed-runner";
import { adoptProductionCompiler, adoptProcessingPanel } from "./pixel-cockpit/processing-integration";
import { compile } from "@tailwindcss/node";
import { Scanner } from "@tailwindcss/oxide";
import { build, type BunPlugin } from "bun";

const root = resolve(import.meta.dir, "..");
const maintainedResource = join(root, "src-tauri/resources/pixel-cockpit");
const resource = process.env.JARVIS_PIXEL_BUILD_OUTPUT ? resolve(process.env.JARVIS_PIXEL_BUILD_OUTPUT) : maintainedResource;
const original = join(root, "docs/image-cockpit-for-codex-workflows");
const upstream = join(maintainedResource, "upstream");
const generated = join(resource, "generated");
const worker = join(resource, "worker");
const adopt = process.argv.includes("--adopt");

async function files(directory: string): Promise<string[]> {
  return (await Promise.all((await readdir(directory, { withFileTypes: true })).map(async (entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  }))).flat();
}

await mkdir(resource, { recursive: true });
if (adopt) {
  const selected = [
    ...await files(join(original, "src")),
    ...await files(join(original, "server")),
    ...await files(join(original, "docs/prompt-examples")),
    ...await files(join(original, "public")),
    join(original, "LICENSE"),
    join(original, "package.json"),
    join(original, "README.md"),
  ];
  const hashes: Record<string, string> = {};
  for (const path of selected) {
    const name = relative(original, path).replaceAll("\\", "/");
    if (name.includes(".test.") || name.endsWith("main.tsx")) continue;
    const content = await readFile(path);
    hashes[name] = createHash("sha256").update(content).digest("hex");
    const destination = join(upstream, name);
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, content);
  }
  await writeFile(join(maintainedResource, "PROVENANCE.json"), JSON.stringify({
    project: "image-cockpit-for-codex-workflows",
    upstreamVersion: "0.2.1",
    license: "MIT",
    copyright: "2026 dreiachse-cyber",
    adoptedFrom: "docs/image-cockpit-for-codex-workflows",
    excluded: [".env", "codex-handoff", "authentication", "browser history", "game artwork"],
    files: hashes,
  }, null, 2));
  await cp(join(original, "public"), join(upstream, "public"), { recursive: true });
}
await mkdir(generated, { recursive: true });
await mkdir(worker, { recursive: true });
await mkdir(join(worker, "fonts"), { recursive: true });
for (const [family, weights] of [["roboto", [400, 500, 600]], ["jetbrains-mono", [400]]] as const) {
  await cp(join(root, "node_modules/@fontsource", family, "LICENSE"), join(worker, "fonts", `${family}-LICENSE.txt`));
  for (const weight of weights) {
    const name = `${family}-latin-${weight}-normal.woff2`;
    await cp(join(root, "node_modules/@fontsource", family, "files", name), join(worker, "fonts", name));
  }
}
await cp(join(upstream, "public"), worker, { recursive: true });
await cp(join(upstream, "src"), join(generated, "src"), { recursive: true });
await cp(join(upstream, "server"), join(generated, "server"), { recursive: true });
await cp(join(upstream, "docs"), join(generated, "docs"), { recursive: true });
const originalApp = await readFile(join(upstream, "src/App.tsx"), "utf8");
const controller = extractController(originalApp);
const localized = localizePortugueseCopy(adoptManagedHealth(controller.source))
  .replace('"pt-BR": withWorkflowCopy({', `"pt-BR": withWorkflowCopy({
    "effect-animation": {label:"Animação de efeitos",detail:"Gere efeitos animados com quadros, transparência e metadados.",status:"Geração de efeitos animados"},`)
  .replace('"pt-BR": { "local-file": "Arquivo local", "local-generator": "Gerador local", "codex-handoff": "Handoff Codex"', '"pt-BR": { "local-file": "Arquivo local", "local-generator": "Gerador local", "codex-handoff": "Geração com Codex"');
const withHonestProgress = localized.replace(
  'const phaseLabel = codexProgressPhaseLabel(labels, phase);',
  'const phaseLabel = `${codexProgressPhaseLabel(labels, phase)} · estimativa de tempo`;'
);
const timeline = exposeSuiteController(withHonestProgress, "AnimationTimelineEditorModal", "timeline", false);
const trackedSource = forwardPublicOperationErrors(timeline.source, publicAppMethods)
  .replace('import { bindController, getRecoverableJob, reportOperationError }', 'import { bindController, getRecoverableJob, reportOperationError, beginManualGeneration }')
  .replace("handleGenerateInFlightRef.current = true;", "handleGenerateInFlightRef.current = true; beginManualGeneration();");
const displayedTournament = trackedSource
  .replace('className={`tournament-monitor-card state-${manifest.state}`}', 'className={`tournament-monitor-card state-${stoppedTournamentOutcome(manifest, codexJobs) ?? manifest.state}`}')
  .replace('<em>{manifest.state} · {formatDurationSinceAt(manifest.createdAt, Date.now())}</em>', '<em>{stoppedTournamentOutcome(manifest, codexJobs) ?? manifest.state} · {formatDurationSinceAt(manifest.createdAt, Date.now())}</em>')
  .replace('<div className="tournament-candidate-grid">', `<div role="status">{stoppedTournamentOutcome(manifest, codexJobs) === "failed" && <><strong>Bloqueado na verificação de qualidade</strong>{manifest.candidates.filter((candidate) => candidate.state === "failed" && candidate.reason).map((candidate) => <p key={candidate.jobId}>{candidate.reason}</p>)}</>}</div><div className="tournament-candidate-grid">`);
await writeFile(join(generated, "src/App.tsx"), 'import { stoppedTournamentOutcome } from "../execution-state";\n' + adoptProcessingPanel(adoptUi(displayedTournament)));
const motionPath = join(generated, "src/lib/motionRecipes.ts");
await writeFile(motionPath, adoptProductionCompiler(await readFile(motionPath, "utf8")));
await writeFile(join(generated, "jarvis-presentation.ts"), presentationRuntimeSource() + "\nexport { jarvisUiText as localizePixelText };\n");
const suites: Record<string, typeof controller.catalog> = { timeline: timeline.catalog };
for (const [name, key] of [["AnimationReviewCockpit", "review"], ["VfxCompositeStage", "vfx"]]) {
  const path = join(generated, "src", `${name}.tsx`);
  const exposed = exposeSuiteController(await readFile(path, "utf8"), name, key);
  suites[key] = exposed.catalog;
  const withErrors = exposed.source
    .replace('import { bindController }', 'import { bindController, reportOperationError }')
    .replace('setMessage(error instanceof Error ? error.message : "Composite export failed");', 'setMessage(error instanceof Error ? error.message : "Composite export failed"); reportOperationError(error);')
    .replace('setMessage(error instanceof Error ? error.message : "Composite import failed");', 'setMessage(error instanceof Error ? error.message : "Composite import failed"); reportOperationError(error);')
    .replace('} catch {\n      setSaveState("error");', '} catch (error) {\n      setSaveState("error"); reportOperationError(error);')
    .replace('setActionError(error instanceof Error ? error.message : "Candidate adoption failed.");', 'setActionError(error instanceof Error ? error.message : "Candidate adoption failed."); reportOperationError(error);')
    .replace('setActionError(error instanceof Error ? error.message : "Direction Repair could not start.");', 'setActionError(error instanceof Error ? error.message : "Direction Repair could not start."); reportOperationError(error);');
  await writeFile(path, adoptUi(withErrors));
}
const imagePath = join(generated, "src/lib/image.ts");
const imageSource = (await readFile(imagePath, "utf8")).replace(
  'export function downloadBlob(blob: Blob, filename: string) {',
  'export function downloadBlob(blob: Blob, filename: string) {\n  if (globalThis.__jarvisPixelDownload) { globalThis.__jarvisPixelDownload(blob, filename); return; }',
);
await writeFile(imagePath, imageSource);
const storagePath = join(generated, "src/lib/storage.ts");
const storageSource = (await readFile(storagePath, "utf8")).replace(
  "export async function loadPersistedState(fallbackActions: SpriteAction[], options: LoadPersistedStateOptions = {}) {",
  `export async function loadPersistedState(fallbackActions: SpriteAction[], options: LoadPersistedStateOptions = {}) {
  const restored = globalThis.__jarvisPixelRestored;
  if (restored && typeof restored === "object" && "history" in restored && Array.isArray(restored.history)) return restored;
`,
);
await writeFile(storagePath, storageSource);
const modules: Record<string, ReturnType<typeof exportedModuleCatalog>> = {};
const imports: string[] = [];
const entries: string[] = [];
const types: Record<string, {source: string; definition: string}[]> = {};
for (const path of await files(join(upstream, "src"))) {
  if (!/\.tsx?$/.test(path) || path.includes(".test.")) continue;
  for (const [name, definition] of Object.entries(typeDeclarationCatalog(await readFile(path, "utf8")))) {
    (types[name] ??= []).push({source:relative(upstream,path).replaceAll("\\","/"),definition});
  }
}
for (const path of await files(join(upstream, "src/lib"))) {
  if (!path.endsWith(".ts") || path.endsWith(".d.ts") || path.includes(".test.")) continue;
  const name = relative(join(upstream, "src/lib"), path).replaceAll("\\", "/").replace(/\.ts$/, "");
  const moduleId = `module${entries.length}`;
  modules[name] = exportedModuleCatalog(await readFile(path, "utf8"));
  imports.push(`import * as ${moduleId} from "./src/lib/${name}";`);
  entries.push(`${JSON.stringify(name)}: ${moduleId}`);
}
await writeFile(join(generated, "modules.ts"), `${imports.join("\n")}\nexport const modules = {${entries.join(",")}};`);
await writeFile(join(generated, "catalog.json"), JSON.stringify({ ...controller.catalog, modules, suites, types }));
await cp(join(root, "scripts/pixel-cockpit/jarvis-worker.ts.template"), join(generated, "jarvis-worker.ts"));
for (const name of ["server-bridge.ts", "contract.ts", "execution-state.ts", "managed-runner.ts", "session-persistence.ts", "production-recipes.ts", "processing-tools.ts"]) {
  await cp(join(root, "scripts/pixel-cockpit", name), join(generated, name));
}
const serverPath = join(generated, "server/index.ts");
let serverSource = adoptManagedRunner(await readFile(serverPath, "utf8"));
serverSource = 'import { handleJarvisRoute, reportJarvisReady } from "../server-bridge";\n' + serverSource;
serverSource = serverSource.replace('  response.setHeader("Access-Control-Allow-Origin", "*");', '  if (await handleJarvisRoute(request, response)) return;');
serverSource = serverSource.replace(
  '  console.log(`Image Cockpit local handoff server listening on http://127.0.0.1:${port}`);',
  '  reportJarvisReady(server.address());',
);
await writeFile(serverPath, serverSource);
await writeFile(join(generated, "main.tsx"), `
import React from "react";
import { createRoot } from "react-dom/client";
import { restoreSession } from "./jarvis-worker";
import App from "./src/App";
await restoreSession();
createRoot(document.getElementById("root")!).render(<App />);
`);
const rawPlugin: BunPlugin = {
  name: "upstream-raw-docs",
  setup(build) {
    build.onResolve({ filter: /\?raw$/ }, ({ path, resolveDir }) => ({ path: resolve(resolveDir, path), namespace: "raw" }));
    build.onLoad({ filter: /.*/, namespace: "raw" }, async ({ path }) => ({
      contents: `export default ${JSON.stringify(await readFile(path.replace(/\?raw$/, ""), "utf8"))}`,
      loader: "js",
    }));
  },
};
const browser = await build({
  entrypoints: [join(generated, "main.tsx")],
  outdir: worker,
  naming: "worker.js",
  target: "browser",
  minify: false,
  plugins: [rawPlugin],
  define: { "process.env.NODE_ENV": '"production"', "import.meta.env.DEV": "false" },
});
if (!browser.success) throw new AggregateError(browser.logs, "Pixel Cockpit worker compilation failed");
const server = await build({
  entrypoints: [serverPath],
  outdir: resource,
  naming: "server.mjs",
  target: "node",
  format: "esm",
});
if (!server.success) throw new AggregateError(server.logs, "Pixel Cockpit server compilation failed");
const utilityCss = await compile(`@import "tailwindcss" source(none);
@theme inline {
 --color-primary:#0071e3; --color-primary-foreground:#fff; --color-background:#242526;
 --color-foreground:#ededed; --color-muted:#343637; --color-muted-foreground:#b0b3b7;
 --color-border:rgb(255 255 255 / 9%); --color-input:rgb(255 255 255 / 18%);
 --color-ring:#69b0ff; --color-destructive:#ff9096; --radius-md:8px;
}`, { base: root, onDependency: () => {} });
const scanner = new Scanner({ sources: [
  { base: join(root, "src/components/ui"), pattern: "{button,input,textarea,checkbox,native-select,dialog,collapsible}.tsx", negated: false },
  { base: join(root, "src/components/pixel-art"), pattern: "PixelProcessingPanel.tsx", negated: false },
  { base: join(generated, "src"), pattern: "**/*.tsx", negated: false },
] });
await writeFile(join(worker, "ui.css"), utilityCss.build(scanner.scan()));
await writeFile(join(worker, "index.html"), '<!doctype html><html lang="pt-BR" class="dark"><head><meta charset="utf-8"><title>Jarvis Pixel Art</title><link rel="stylesheet" href="/ui.css"><link rel="stylesheet" href="/worker.css"><link rel="stylesheet" href="/jarvis-theme.css"></head><body><div id="root"></div><script type="module" src="/worker.js"></script></body></html>');
await writeFile(join(worker, "worker.css"), adoptTheme(await readFile(join(upstream, "src/styles.css"), "utf8")));
await cp(join(root, "scripts/pixel-cockpit/jarvis-theme.css"), join(worker, "jarvis-theme.css"));
await cp(join(upstream, "LICENSE"), join(resource, "LICENSE"));
console.log(JSON.stringify({ resource, methods: Object.keys(controller.catalog.methods).length, settings: Object.keys(controller.catalog.settings).length, modules: Object.keys(modules).length }));
