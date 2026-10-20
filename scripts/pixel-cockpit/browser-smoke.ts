import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "../..");
const report = join(root, ".codex/pixel-cockpit-browser", new Date().toISOString().replace(/[:.]/g, "-"));
const resource = process.env.JARVIS_PIXEL_TEST_RESOURCE ?? join(root, "src-tauri/resources/pixel-cockpit");
await mkdir(report, { recursive: true });
const token = randomUUID().replaceAll("-", "");
const session = join(report, "session");
const server = spawn("node", [join(resource, "server.mjs")], {
  cwd: report, windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
  env: { ...process.env, IMAGE_COCKPIT_API_PORT: "0", IMAGE_COCKPIT_HANDOFF_DIR: session,
    JARVIS_PIXEL_RESOURCE_DIR: resource, JARVIS_PIXEL_SESSION_TOKEN: token, JARVIS_PIXEL_WORKSPACE: root },
});
const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
let stderr = "";
server.stderr?.on("data", (chunk) => { stderr += String(chunk); });
const ready = await new Promise<{ origin: string }>((resolve, reject) => {
  let output = "";
  const timer = setTimeout(() => reject(new Error(`Server readiness timed out: ${stderr}`)), 15000);
  server.stdout?.on("data", (chunk) => {
    output += String(chunk);
    for (const line of output.split(/\r?\n/)) {
      if (!line.startsWith("{")) continue;
      try {
        const value = JSON.parse(line) as { type?: string; origin?: string };
        if (value.type === "ready" && value.origin) { clearTimeout(timer); resolve({ origin: value.origin }); }
      } catch { /* The next data event completes a split line. */ }
    }
  });
  server.once("exit", (code) => { clearTimeout(timer); reject(new Error(`Server exited ${code}: ${stderr}`)); });
});
const browser = spawn(process.env.JARVIS_PIXEL_TEST_BROWSER ?? "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", [
  "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
  "--remote-debugging-port=0", "--remote-debugging-address=127.0.0.1",
  `--user-data-dir=${join(report, "browser-profile")}`, "about:blank",
], { windowsHide: true, stdio: "ignore" });
let socket: WebSocket | undefined;
const evidence: unknown[] = [];
try {
  let debugPort = "";
  for (let attempt = 0; attempt < 100; attempt++) {
    try { debugPort = (await readFile(join(report, "browser-profile/DevToolsActivePort"), "utf8")).split(/\r?\n/)[0]; break; }
    catch { await pause(100); }
  }
  if (!debugPort) throw new Error("Browser did not start its test endpoint.");
  const target = await (await fetch(`http://127.0.0.1:${debugPort}/json/new?${encodeURIComponent(`${ready.origin}/?token=${token}`)}`, { method: "PUT" })).json() as { webSocketDebuggerUrl: string };
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise<void>((resolve, reject) => {
    socket!.onopen = () => resolve();
    socket!.onerror = () => reject(new Error("Browser CDP failed to connect."));
  });
  let nextId = 0;
  const pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
  socket.onmessage = (event) => {
    const data = JSON.parse(String(event.data)) as { id?: number; result?: unknown; error?: { message: string }; method?: string; params?: unknown };
    if (!data.id) {
      if (data.method === "Runtime.exceptionThrown") evidence.push({ browserException: data.params });
      return;
    }
    const request = pending.get(data.id);
    if (!request) return;
    pending.delete(data.id);
    if (data.error) request.reject(new Error(data.error.message));
    else request.resolve(data.result);
  };
  const command = (method: string, params: unknown) => new Promise<unknown>((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, { resolve, reject });
    socket!.send(JSON.stringify({ id, method, params }));
  });
  await command("Runtime.enable", {});
  await command("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
  const evaluate = async (expression: string) => {
    const response = await command("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }) as { result: { value?: unknown }; exceptionDetails?: unknown };
    if (response.exceptionDetails) throw new Error(JSON.stringify(response.exceptionDetails));
    return response.result.value;
  };
  for (let attempt = 0; attempt < 150; attempt++) {
    const status = await evaluate("globalThis.__jarvisPixel?.snapshot()") as { ready?: boolean } | undefined;
    if (status?.ready) break;
    if (attempt === 149) throw new Error("Original suite did not bind its controller.");
    await pause(100);
  }
  const execute = async (action: string, args: unknown = {}, expectedStatus = "completed") => {
    const id = randomUUID().replaceAll("-", "");
    await evaluate(`globalThis.__jarvisPixel.start(${JSON.stringify({ id, action, args })})`);
    for (let attempt = 0; attempt < 300; attempt++) {
      const result = await evaluate(`globalThis.__jarvisPixel.snapshot(${JSON.stringify(id)})`) as { status: string; error?: string; result?: unknown; artifacts?: unknown[] };
      if (result.status !== "running") {
        evidence.push({ action, args, ...result });
        if (result.status !== expectedStatus) throw new Error(`${action}: ${result.error}`);
        return result;
      }
      await pause(100);
    }
    throw new Error(`${action} timed out.`);
  };
  const interruptedJobId = "codex-job-interrupted-regression";
  await writeFile(join(session, "status", `${interruptedJobId}.json`), JSON.stringify({
    jobId:interruptedJobId,state:"running",message:"Fixture: executor lost before publishing output",
    startedAt:"2026-01-01T00:00:00Z",
  }));
  for (let attempt = 0; attempt < 2; attempt++) {
    const observed = await evaluate(`fetch("/api/codex/jobs/${interruptedJobId}/status").then(response=>response.json())`) as {status:{state:string;resumeRequired?:boolean;resumeCount?:number}};
    if (observed.status.state !== "failed" || !observed.status.resumeRequired || observed.status.resumeCount) throw new Error("A read-only job status attempted to resume the interrupted executor.");
    evidence.push({action:"observe_interrupted_fixture",...observed});
  }
  await execute("invoke",{method:"resumeInterruptedCodexJob",args:[interruptedJobId,"process_existing"]},"failed");
  if (process.argv.includes("--maintenance-only")) {
    await execute("import_image", {path:".codex/pixel-review/a19cda3ac28068bf1033f1be02f8cda9/project/fixtures/source.png"});
    await execute("invoke", {method:"usePromptExample",args:[{state:"promptExamples.0"}]});
    await execute("invoke", {method:"useQuickPrompt",args:["Verification uses the existing imported robot; do not generate."]});
    await execute("invoke", {method:"beginWorkflow",args:["image-edit"]});
    await execute("invoke", {method:"selectSourceFromPreview",args:[{state:"selected"}]});
    await execute("invoke", {method:"selectHistoryResult",args:[{state:"selected"}]});
    await execute("invoke", {method:"adoptSelected",args:[]});
    await execute("invoke", {method:"useAnimationPresetExample",args:[{state:"animationPresets.0"}]});
    await execute("invoke", {method:"toggleFavoriteAnimationPreset",args:["walk-cycle"]});
    await execute("invoke", {method:"toggleFavoriteAnimationPreset",args:["walk-cycle"]});
    await execute("invoke", {method:"selectAnimationBodyTopology",args:["quadruped"]},"failed");
    await execute("configure", {settings:{animationBodyTopology:"quadruped"}},"failed");
    const preserved = await execute("state");
    if ((preserved.result as {animationBodyTopology:string}).animationBodyTopology !== "biped") throw new Error("Invalid topology mutated the selected recipe.");
    await execute("preset",{kind:"animation",id:"dash"});
    await execute("invoke",{method:"selectAnimationBodyTopology",args:["quadruped"]});
    await execute("invoke",{method:"selectAnimationBodyTopology",args:["biped"]});
    await execute("state");
  } else if (!process.argv.includes("--recovery-only")) {
  await execute("catalog", { query: "timeline" });
  const detailed = await execute("catalog", {module:"animationPackV2",method:"moveAnimationEvent"});
  if (!(detailed.result as {schema?:unknown;relatedTypes?:Record<string,unknown>}).schema
    || !(detailed.result as {relatedTypes?:Record<string,unknown>}).relatedTypes?.AnimationPackV2) throw new Error("Advanced method catalog lost its original argument types.");
  const effectSettings = await execute("configure", {settings:{
    effectFrameCount:4,effectCanvasSize:96,effectCategoryId:"hit-spark",effectTypeId:"burst",effectLoopMode:"loop",
  }});
  const actualEffect = effectSettings.result as {effectFrameCount:number;effectCanvasSize:number;effectLoopMode:string};
  if(actualEffect.effectFrameCount !== 4 || actualEffect.effectCanvasSize !== 96 || actualEffect.effectLoopMode !== "loop") {
    throw new Error("Effect preset overwrote the requested frame count, canvas size or loop mode.");
  }
  await execute("configure", { settings: { prompt: "Runtime processing verification", language: "pt-BR" } });
  await execute("import_image", { path: "docs/image-cockpit-for-codex-workflows/public/samples/forest-mage-sheet.png" });
  await execute("inspect");
  await execute("invoke", {method:"runCockpitHealthCheck"});
  await execute("invoke", {method:"repairCockpit",confirm:true});
  await execute("invoke", {method:"exportEnvironmentReport",args:["json"]});
  await execute("invoke", {method:"exportEnvironmentReport",args:["markdown"]});
  const annotation = await execute("annotate", { rect: { x: 0.1, y: 0.1, width: 0.3, height: 0.3 }, comment: "Região de teste" });
  await execute("annotate", { tool: "arrow", points: [{ x: 0.1, y: 0.1 }, { x: 0.3, y: 0.3 }], comment: "Seta de teste" });
  await execute("annotate", { tool: "brush", points: [{ x: 0.1, y: 0.1 }, { x: 0.2, y: 0.2 }, {x: 0.3, y: 0.2}], comment: "Traço de teste" });
  await execute("invoke", { method: "updateAnnotationComment", args: [(annotation.result as {id: string}).id, "Comentário corrigido"] });
  await execute("invoke", { method: "removeAnnotation", args: [(annotation.result as {id: string}).id] });
  await execute("invoke", { method: "clearSelectedAnnotations" });
  await execute("configure", { settings: { prompt: "Invalid change must be atomic", count: 99 } }, "failed");
  const unchanged = await execute("state");
  if ((unchanged.result as {prompt: string}).prompt !== "Runtime processing verification") throw new Error("Invalid configuration partially changed state.");
  await execute("invoke", { method: "splitSelectedToTimeline", args: [] });
  await execute("invoke", { method: "addSelectedAsFrame", args: [] });
  await execute("invoke", { method: "moveFrame", args: [{state: "activeAction.frameIds.0"}, 1] });
  await execute("configure", { settings: { selectedFrameId: {state: "activeAction.frameIds.0"}, annotationColor: "#00ff00" } });
  await execute("invoke", { method: "applyChromaToSelectedFrame" });
  await execute("invoke", { method: "removeFrame", args: [{state: "activeAction.frameIds.0"}] });
  await execute("invoke", { method: "updateCell", args: [256,256] });
  await execute("invoke", { method: "updateActiveAction", args: [{fps:8,loop:true}] });
  await execute("export", { format: "image" });
  const screenshot = await command("Page.captureScreenshot", { format: "png", captureBeyondViewport: false }) as { data: string };
  await writeFile(join(report, "suite.png"), Buffer.from(screenshot.data, "base64"));
  const library = await execute("invoke", { module: "officialAnimations", method: "OFFICIAL_ANIMATION_LIBRARY" });
  const sample = (library.result as {id: string}[]).find((item) => item.id === "official-walk-cycle");
  if (!sample) throw new Error("Original sample animation is absent.");
  await execute("invoke", { method: "useAnimationLibraryItem", args: [sample] });
  const dynamicCopy = await evaluate("document.body.innerText") as string;
  evidence.push({action:"dynamic_translation",text:dynamicCopy});
  if (!dynamicCopy.includes("Suíte funcionando.") || !dynamicCopy.includes("Hospedagem gerenciada pelo Jarvis")
    || dynamicCopy.includes("Cockpit OK.") || dynamicCopy.includes("Supervisor off")
    || dynamicCopy.includes("Stable 8-frame") || / • import(?:\r?\n|$)/.test(dynamicCopy)) {
    throw new Error("Dynamic recipe, health or history presentation remains untranslated.");
  }
  const presentation = await evaluate(`(async () => {
    const fonts = ["roboto-latin-400-normal.woff2","roboto-latin-500-normal.woff2","roboto-latin-600-normal.woff2","jetbrains-mono-latin-400-normal.woff2"];
    const fontResponses = await Promise.all(fonts.map(async (name) => { const response = await fetch("/fonts/" + name); return {name,status:response.status,mime:response.headers.get("content-type"),bytes:(await response.arrayBuffer()).byteLength}; }));
    await document.fonts.load('400 13px "Roboto"');
    await document.fonts.load('400 11px "JetBrains Mono"');
    const cards = [...document.querySelectorAll(".history-item")].map((element) => element.getBoundingClientRect().height);
    const selected = document.querySelector(".history-item.selected");
    return {fonts:fontResponses,cards,selectedBackground:selected ? getComputedStyle(selected).backgroundColor : null};
  })()`) as {fonts:{status:number;mime:string;bytes:number}[];cards:number[];selectedBackground:string|null};
  if(presentation.fonts.some((font)=>font.status!==200 || font.mime!=="font/woff2" || font.bytes<1000)) throw new Error("Bundled interface fonts did not load.");
  if(presentation.cards.length<2 || presentation.cards.some((height)=>height>=140)) throw new Error("History cards stretched instead of keeping a compact height.");
  if(presentation.selectedBackground !== "rgb(64, 67, 69)") throw new Error("History selection did not use the Jarvis neutral accent.");
  evidence.push({action:"presentation_checks",...presentation});
  await execute("timeline_open");
  await execute("configure", { controller: "timeline", settings: { playing: false, frameIndex: 2, speed: 0.5 } });
  await execute("invoke", { module: "animationPackV2", method: "setAnimationFrameDuration", args: [{state: "draft", controller: "timeline"}, 0, 150], saveAs: "edited-timeline" });
  await execute("invoke", { controller: "timeline", method: "commit", args: [{saved: "edited-timeline"}] });
  await execute("invoke", { controller: "timeline", method: "save" });
  await execute("invoke", { controller: "timeline", method: "roundTrip" });
  const timelineCopy = await evaluate('document.querySelector("[data-jarvis-suite-dialog]")?.textContent') as string;
  if(!timelineCopy.includes("Quadro selecionado") || timelineCopy.includes("Selected frame") || timelineCopy.includes("round-trip: identical")) {
    throw new Error("Timeline interface still contains untranslated presentation copy.");
  }
  const pack = await execute("invoke", { controller: "timeline", method: "exportPack" });
  const timelineScreenshot = await command("Page.captureScreenshot", { format: "png", captureBeyondViewport: false }) as { data: string };
  await writeFile(join(report, "timeline.png"), Buffer.from(timelineScreenshot.data, "base64"));
  await execute("invoke", { controller: "timeline", method: "close" });
  await execute("state", { controller: "timeline" }, "failed");
  const packPath = (pack.artifacts as {path: string}[])[0]?.path;
  if (!packPath) throw new Error("Timeline did not export its pack.");
  await execute("import_animation_pack", {path:packPath});
  const imported = await execute("state");
  const item = (imported.result as {userAnimationLibrary: {id:string}[]}).userAnimationLibrary[0];
  if (!item) throw new Error("The animation pack did not enter the reusable library.");
  await execute("invoke", {method:"renameUserAnimationItem",args:[item.id,"Verification Walk"]});
  await execute("invoke", {method:"exportAnimationLibraryItem",args:[{ref:"userAnimationLibrary",id:item.id}]});
  await execute("invoke", {method:"deleteUserAnimationItem",args:[{ref:"userAnimationLibrary",id:item.id}]});
  const deleted = await execute("state");
  if ((deleted.result as {userAnimationLibrary:{id:string}[]}).userAnimationLibrary.some((entry)=>entry.id===item.id)) throw new Error("Library delete did not remove the requested item.");
  await pause(500);
  await command("Page.reload", {});
  for (let attempt = 0; attempt < 150; attempt++) {
    if ((await evaluate("globalThis.__jarvisPixel?.snapshot()") as {ready?:boolean}|undefined)?.ready) break;
    await pause(100);
  }
  const restored = await execute("state");
  if (!(restored.result as {history:unknown[]}).history.length) throw new Error("Session history was lost after reload.");
  if((restored.result as {effectFrameCount:number}).effectFrameCount !== 4
    || (restored.result as {effectCanvasSize:number}).effectCanvasSize !== 96) throw new Error("Effect settings changed after restoring the session.");
  await execute("configure", {settings:{workflowMode:"image-generate",providerId:"local-generator",prompt:"Manual activity publication verification",count:1}});
  const previousManual = await evaluate("globalThis.__jarvisPixel.activity()") as {id:string}[];
  const clicked = await evaluate(`(() => { const button = document.querySelector('.primary-action-row button.primary-button'); if (!(button instanceof HTMLButtonElement) || button.disabled) return false; button.click(); return true; })()`);
  if (!clicked) throw new Error("The manual generation control is unavailable.");
  let manualReceipt: {id:string;status:string;artifacts?:unknown[];error?:string} | undefined;
  for (let attempt = 0; attempt < 300; attempt++) {
    const activity = await evaluate("globalThis.__jarvisPixel.activity()") as typeof manualReceipt[];
    manualReceipt = activity.find((item) => item && !previousManual.some((old) => old.id === item.id));
    if (manualReceipt && manualReceipt.status !== "running") break;
    await pause(100);
  }
  if (manualReceipt?.status !== "completed" || !manualReceipt.artifacts?.length) throw new Error(`Manual generation was not published: ${manualReceipt?.error ?? manualReceipt?.status}`);
  evidence.push({action:"manual_generate",source:"real suite button; explicit procedural provider only",...manualReceipt});
  const manualCount = (await evaluate("globalThis.__jarvisPixel.activity()") as unknown[]).length;
  await execute("generate", {workflow:"image-generate",provider:"local-generator",parameters:{prompt:"Worker activity deduplication verification",count:1}});
  const afterWorker = await evaluate("globalThis.__jarvisPixel.activity()") as unknown[];
  if (afterWorker.length !== manualCount) throw new Error("The worker generation also created a duplicate manual receipt.");
  }
  await writeFile(join(report, "evidence.json"), JSON.stringify({ scope: "Real browser processing and explicit procedural-provider activity checks; no AI generation quality claimed", results: evidence }, null, 2));
  console.log(JSON.stringify({ passed: true, report, operations: evidence.length }));
} catch (error) {
  await writeFile(join(report, "evidence.json"), JSON.stringify({
    passed: false,
    scope: "Real browser processing and explicit procedural-provider activity checks; no AI generation quality claimed",
    error: error instanceof Error ? error.message : String(error),
    server: {exitCode:server.exitCode,signalCode:server.signalCode,stderr},
    results: evidence,
  }, null, 2));
  throw error;
} finally {
  socket?.close();
  for (const process of [browser, server] as ChildProcess[]) {
    if (!process.pid) continue;
    await new Promise<void>((resolve) => {
      const stop = spawn("taskkill.exe", ["/PID", String(process.pid), "/T", "/F"], { windowsHide: true, stdio: "ignore" });
      stop.on("exit", () => resolve());
    });
  }
}
