import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { MessageChannel } from "node:worker_threads";
import { expect, it } from "vitest";
import ts from "typescript";
import * as contract from "./contract";
import * as executionState from "./execution-state";
import { createSnapshotWriter } from "./session-persistence";
import { presentationRuntimeSource } from "./ui-adapter";

type Data = Record<string, unknown>;
type Receipt = { status: string; phase: string; finishedAt?: string; error?: string };
const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
const displayRuntime = ts.transpileModule(presentationRuntimeSource(), {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const localizePixelText = new Function(displayRuntime + "; return jarvisUiText;")() as (value: string) => string;
async function until(predicate: () => boolean) {
  const deadline = Date.now() + 2500;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error("Expected worker condition was not reached.");
    await pause(10);
  }
}

async function worker(restored: Data = {}, hiddenTimers = false) {
  // The original callback boundary is controlled here; no model or process is launched.
  const source = await readFile(process.env.JARVIS_PIXEL_CANCELLATION_SOURCE
    ?? "scripts/pixel-cockpit/jarvis-worker.ts.template", "utf8");
  const compiled = ts.transpileModule(source.replace(/^import .+;\r?\n/gm, "").replace(/^export /gm, ""), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
  }).outputText;
  const context = {
    ...contract, ...executionState, createSnapshotWriter, localizePixelText, catalogDefinition: {}, modules: {},
    window: { confirm: () => false }, localStorage: { getItem: () => null, setItem: () => undefined },
    Blob, File, console, MessageChannel,
    setTimeout: hiddenTimers ? ((callback: () => void, delay: number) => delay === 35 ? 0 : setTimeout(callback, delay)) : setTimeout,
    clearTimeout, btoa,
    fetch: async (path: string, options?: {body?:string}) => ({
      ok: true, json: async () => {
        if (path !== "/jarvis/artifacts") return restored;
        const data = JSON.parse(options?.body ?? "{}") as {name:string;mime:string};
        return {path:`artifacts/${data.name}`,name:data.name,mime:data.mime};
      },
      blob: async () => new Blob(["fixture"], {type:"image/png"}),
    }),
    bindController: undefined as unknown as (controller: unknown) => void,
    restoreSession: undefined as unknown as () => Promise<void>,
    getRecoverableJob: undefined as unknown as (id: string) => Data | undefined,
    __jarvisPixelDownload: undefined as unknown as (blob: Blob, name: string) => void,
    __jarvisPixel: undefined as unknown as {
      start: (request: { id: string; action: string; args: Data }) => Receipt;
      snapshot: (id: string) => Receipt;
      cancel: (id: string) => void;
    },
  };
  runInNewContext(compiled, context);
  await context.restoreSession();
  const state: Data = {
    storageHydrated: false, history: [], codexJobs: [], codexJobLogs: [],
    providerId: "codex-handoff", workflowMode: "image-generate", isBusy: false,
  };
  const methods: Record<string, (...args: unknown[]) => unknown> = {
    beginWorkflow: (mode) => { state.workflowMode = mode; },
  };
  const setters = {
    workflowMode: (value: unknown) => { state.workflowMode = value; },
    providerId: (value: unknown) => { state.providerId = value; },
  };
  context.bindController({state, methods, setters});
  return { bridge: context.__jarvisPixel, state, methods, setters, getRecoverableJob: context.getRecoverableJob, download:context.__jarvisPixelDownload };
}

it("applies suite settings while hidden-window timers are suspended", async () => {
  const { bridge, state } = await worker({}, true);
  bridge.start({id:"hidden-settings", action:"configure", args:{settings:{workflowMode:"image-edit"}}});
  await until(() => Boolean(bridge.snapshot("hidden-settings").finishedAt));
  expect(bridge.snapshot("hidden-settings").status).toBe("completed");
  expect(state.workflowMode).toBe("image-edit");
});

it("restores the exact interrupted receipt and job metadata without resubmitting it", async () => {
  const { bridge, state, getRecoverableJob } = await worker({
    operations: [{id:"interrupted", action:"generate", status:"running", logs:[], artifacts:[], startedAt:"2026-01-01"}],
    operationJobs: {interrupted:["original-job"]},
    pendingJobs: [{id:"original-job", workflowMode:"effect-animation", effectContext:{frameCount:4,canvasSize:96}}],
  });
  expect(bridge.snapshot("interrupted")).toMatchObject({
    status:"blocked", recoveryRequired:true, originalJobIds:["original-job"],
  });
  expect(state.codexJobs).toEqual([]);
  expect(getRecoverableJob("original-job")?.effectContext).toEqual({frameCount:4,canvasSize:96});
  expect(bridge.snapshot("unrelated")).toBeNull();
});

it("refuses an explicit recovery method without confirmation", async () => {
  const { bridge, methods } = await worker();
  let invoked = false;
  methods.resumeInterruptedCodexJob = () => { invoked = true; };
  bridge.start({id:"unconfirmed",action:"invoke",args:{method:"resumeInterruptedCodexJob",args:["original-job","process_existing"]}});
  await until(() => Boolean(bridge.snapshot("unconfirmed").finishedAt));
  expect(bridge.snapshot("unconfirmed").error).toContain("confirm=true");
  expect(invoked).toBe(false);
});

it("preserves original queue and candidate records containing reserved reference field names", async () => {
  const {bridge,methods} = await worker();
  const options = {
    job:{id:"codex-real-job",state:"completed",workflowMode:"sprite-generate",path:"inbox/codex-real-job.json",createdAt:"2026-10-20T07:00:00Z"},
    candidate:{id:"candidate-a",state:"failed",reason:"Quality check rejected subtle movement"},
    exportMetadata:{saved:"completed",id:"archive-1"},
    sourceMetadata:{ref:"history",id:"original",filename:"original.png"},
    fileMetadata:{file:{path:"retained.png"},role:"reference"},
    throwOnError:false,
  };
  let received: unknown;
  methods.importLatestOutboxResult = (value) => {received=value;};
  bridge.start({id:"literal-queue",action:"invoke",args:{method:"importLatestOutboxResult",args:[options]}});
  await until(()=>Boolean(bridge.snapshot("literal-queue").finishedAt));
  expect(bridge.snapshot("literal-queue").status).toBe("completed");
  expect(received).toEqual(options);
});

it("resolves only documented exact state, saved, ref and file wrappers for original method arguments", async () => {
  const {bridge,methods,state} = await worker();
  const job = {id:"retained-job",state:"completed"};
  state.retainedJob = job;
  state.history = [{id:"source-a",name:"source.png"}];
  const received: unknown[] = [];
  methods.importLatestOutboxResult = (...values) => {received.push(values); return job;};
  bridge.start({id:"save-value",action:"invoke",args:{method:"importLatestOutboxResult",args:[{state:"retainedJob",controller:"app"}],saveAs:"saved-job"}});
  await until(()=>Boolean(bridge.snapshot("save-value").finishedAt));
  bridge.start({id:"read-values",action:"invoke",args:{method:"importLatestOutboxResult",args:[
    {saved:"saved-job"},{ref:"history",id:"source-a"},{file:{dataUrl:"data:image/png;base64,AA==",name:"input.png"}},
  ]}});
  await until(()=>Boolean(bridge.snapshot("read-values").finishedAt));
  expect(bridge.snapshot("read-values").status).toBe("completed");
  expect(received[0]).toEqual([job]);
  const values = received[1] as unknown[];
  expect(values.slice(0,2)).toEqual([job,(state.history as unknown[])[0]]);
  expect(values[2]).toBeInstanceOf(File);
  expect((values[2] as File).name).toBe("input.png");
});

it("reconciles only the durably linked single-job receipt after original import succeeds", async () => {
  const { bridge, methods, state } = await worker({
    operations: [
      {id:"previous", action:"generate", status:"running", logs:[], artifacts:[], startedAt:"2026-01-01"},
      {id:"unrelated", action:"generate", status:"blocked", logs:[], artifacts:[], startedAt:"2026-01-01"},
    ],
    operationJobs:{previous:["original-job"],unrelated:["other-job"]},
  });
  methods.resumeInterruptedCodexJob = () => {
    state.history = [{id:"recovered",name:"recovered.png",dataUrl:"data:image/png;base64,AA=="}];
    return {state:"completed"};
  };
  bridge.start({id:"recovery",action:"invoke",args:{method:"resumeInterruptedCodexJob",args:["original-job","process_existing"],confirm:true}});
  await until(() => Boolean(bridge.snapshot("recovery").finishedAt));
  expect(bridge.snapshot("recovery")).toMatchObject({status:"completed"});
  expect(bridge.snapshot("previous")).toMatchObject({
    status:"completed",recoveryRequired:false,originalJobIds:["original-job"],
    artifacts:[{path:"artifacts/recovered.png",name:"recovered.png",mime:"image/png"}],
  });
  expect(bridge.snapshot("unrelated").status).toBe("blocked");
});

it("publishes the original GIF for a recovered effect without starting another generation", async () => {
  const {bridge,state,methods,download} = await worker();
  methods.resumeInterruptedCodexJob = () => {
    state.history = [{id:"effect",name:"effect.png",dataUrl:"data:image/png;base64,AA==",effectAnimation:{frameCount:4}}];
  };
  methods.selectHistoryResult = (item) => {state.selected = item;};
  methods.exportSelectedEffectGif = () => download(new Blob(["GIF89a"],{type:"image/gif"}),"effect.gif");
  bridge.start({id:"effect-recovery",action:"invoke",args:{method:"resumeInterruptedCodexJob",args:["effect-job","process_existing"],confirm:true}});
  await until(() => Boolean(bridge.snapshot("effect-recovery").finishedAt));
  expect(bridge.snapshot("effect-recovery")).toMatchObject({
    status:"completed",artifacts:expect.arrayContaining([{path:"artifacts/effect.gif",name:"effect.gif",mime:"image/gif"}]),
  });
});

it("returns a resumable blocked batch with its confirmed files when a tournament has only failed candidates", async () => {
  const {bridge,state,methods} = await worker();
  let submissions = 0;
  methods.handleGenerate = () => {
    submissions++;
    state.history = [{id:"confirmed-walk",name:"walk.png",dataUrl:"data:image/png;base64,AA=="}];
    state.animationTournamentMonitors = [{manifest:{
      tournamentId:"failed-idle",state:"running",candidates:[{
        jobId:"completed-codex",state:"failed",reason:"Direction split QA failed: idle breathing motion too static across directions (0/1 readable rows, 1.1% average frame change)",
      }],
    }}];
  };
  bridge.start({id:"batch-stale",action:"generate",args:{workflow:"image-generate"}});
  await until(()=>Boolean(bridge.snapshot("batch-stale").finishedAt));
  expect(bridge.snapshot("batch-stale")).toMatchObject({
    status:"blocked",
    phase:"Bloqueado na verificação de qualidade",
    error:expect.stringContaining("A respiração em repouso está muito estática"),
    nextAction:expect.stringContaining("failed-idle"),
    artifacts:expect.arrayContaining([{path:"artifacts/walk.png",name:"walk.png",mime:"image/png"}]),
  });
  expect(submissions).toBe(1);
});

it("never submits generation cancelled while its configuration is still settling", async () => {
  const { bridge, methods, setters } = await worker();
  let submitted = 0;
  methods.handleGenerate = () => { submitted++; };
  const applyWorkflow = setters.workflowMode;
  setters.workflowMode = (value) => { applyWorkflow(value); bridge.cancel("early"); };
  bridge.start({ id: "early", action: "generate", args: { workflow: "image-generate" } });
  await until(() => Boolean(bridge.snapshot("early").finishedAt));
  expect(bridge.snapshot("early").status).toBe("cancelled");
  expect(submitted).toBe(0);
});

it("explains that a valid targeted repair awaits acceptance and preserves the existing directions", async () => {
  const {bridge,state,methods} = await worker();
  const accepted = {tournamentId:"accepted",state:"accepted",candidates:[{jobId:"winner",state:"accepted"}]};
  state.animationTournamentMonitors = [{manifest:accepted}];
  let acceptedRepair = false;
  methods.acceptDirectionRepairFromUi = () => {acceptedRepair = true;};
  methods.startDirectionRepairFromUi = () => {
    state.status = "Direction Repair ready for review: front. Untargeted directions remain locked until acceptance.";
    state.animationTournamentMonitors = [{manifest:{...accepted,candidates:[...accepted.candidates,{
      jobId:"front-repair",state:"quality-evaluated",repairDirections:["front"],score:3400,warningCount:0,
    }]}}];
  };
  bridge.start({id:"front-repair",action:"invoke",args:{method:"startDirectionRepairFromUi",args:[accepted,["front"]]}});
  await until(()=>Boolean(bridge.snapshot("front-repair").finishedAt));
  expect(bridge.snapshot("front-repair")).toMatchObject({
    status:"blocked",
    phase:"Reparo gerado; aguardando aceitação",
    error:"O reparo de Frente foi gerado e está pronto para revisão. As direções não selecionadas continuam preservadas; a publicação aguarda a aceitação do reparo.",
    nextAction:expect.stringContaining("acceptDirectionRepairFromUi"),
    result:{repairReview:{candidate:{jobId:"front-repair",state:"quality-evaluated"}}},
  });
  expect(acceptedRepair).toBe(false);
});

it("returns a translated review receipt without requiring the pilot modal to be open", async () => {
  const {bridge,state,methods} = await worker();
  methods.handleGenerate = () => {
    state.status = "Motion Pilot A/B/C is ready for human review: side. Choose a winner to expand or use Balanced fallback.";
    state.animationTournamentMonitors = [{manifest:{
      tournamentId:"pilot-review",state:"pilot-review",candidates:[
        {jobId:"completed-a",state:"quality-evaluated"},
        {jobId:"completed-b",state:"quality-evaluated"},
        {jobId:"completed-c",state:"quality-evaluated"},
      ],
    }}];
  };
  bridge.start({id:"pilot-review",action:"generate",args:{workflow:"image-generate"}});
  await until(()=>Boolean(bridge.snapshot("pilot-review").finishedAt));
  expect(bridge.snapshot("pilot-review")).toMatchObject({
    status:"blocked",
    phase:"Aguardando revisão dos candidatos",
    error:"Os candidatos precisam de uma decisão de revisão antes da publicação.",
    nextAction:expect.stringContaining("pilot-review"),
  });
});

it("keeps cancellation pending until late submission is stopped and confirmed", async () => {
  const { bridge, methods, state } = await worker();
  let submitting = false;
  let stopped = false;
  let release: () => void = () => undefined;
  const submitted = new Promise<void>((resolve) => { release = resolve; });
  methods.handleGenerate = async () => {
    submitting = true;
    await submitted;
    state.codexJobs = [{ id: "late-job", state: "running" }];
  };
  methods.cancelCodexJob = async (id) => {
    expect(id).toBe("late-job");
    await pause(40);
    stopped = true;
    state.codexJobs = [];
    return { ok: true };
  };
  bridge.start({ id: "late", action: "generate", args: { workflow: "image-generate" } });
  await until(() => submitting);
  bridge.cancel("late");
  await pause(30);
  expect(bridge.snapshot("late").status).toBe("running");
  release();
  await until(() => Boolean(bridge.snapshot("late").finishedAt));
  expect(stopped).toBe(true);
  expect(bridge.snapshot("late").status).toBe("cancelled");
});

it("waits for the original React job publication after submission returns", async () => {
  const { bridge, methods, state } = await worker();
  let stopped = false;
  methods.handleGenerate = async () => {
    bridge.cancel("publication");
    setTimeout(() => { state.codexJobs = [{ id: "late-job", state: "running" }]; }, 20);
  };
  methods.cancelCodexJob = async () => {
    stopped = true;
    state.codexJobs = [];
    return { ok: true };
  };
  bridge.start({ id: "publication", action: "generate", args: { workflow: "image-generate" } });
  await until(() => Boolean(bridge.snapshot("publication").finishedAt));
  expect(stopped).toBe(true);
  expect(bridge.snapshot("publication").status).toBe("cancelled");
});
