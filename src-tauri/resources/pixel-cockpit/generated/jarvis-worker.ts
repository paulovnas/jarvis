import catalogDefinition from "./catalog.json";
import { modules } from "./modules";
import { configurableNames, isPublicModuleMember, publicAppMethods, resolveInvocationArguments, selectCatalogMethod, settingApplicationGroups, suiteSettingNames, validateCallable, validateMotionConfiguration, validateSetting, validateSuiteSetting } from "./contract";
import { executionBaseline, observeExecution, shouldTrackManualGeneration, type ExecutionBaseline } from "./execution-state";
import { localizePixelText } from "./jarvis-presentation";
import { createSnapshotWriter } from "./session-persistence";
import { deriveProcessedAssets, processNativeImage, selectProcessingSource, processingSelection } from "./processing-tools";
import { productionRecipe, productionRecipes } from "./production-recipes";

type Data = Record<string, unknown>;
type Callable = (...args: unknown[]) => unknown;
interface Controller { state: Data; setters: Record<string, Callable>; methods: Record<string, Callable> }
interface Artifact { name: string; path: string; mime: string; size?: number }
interface Operation {
  id: string;
  action: string;
  status: "running" | "completed" | "blocked" | "failed" | "cancelled";
  phase: string;
  progress?: { completed: number; total: number; unit: string };
  logs: string[];
  artifacts: Artifact[];
  result?: unknown;
  error?: string;
  nextAction?: string;
  startedAt: string;
  finishedAt?: string;
  originalJobIds?: string[];
  recoveryRequired?: boolean;
}

const controllers = new Map<string, Controller>();
const operations = new Map<string, Operation>();
const cancellations = new Map<string, Promise<void>>();
const dispatching = new Set<string>();
const operationJobs = new Map<string, Set<string>>();
const recoverableJobs = new Map<string, Data>();
const knownBeforeOperation = new Map<string, Set<string>>();
const values = new Map<string, unknown>();
const pendingDownloads = new Set<Promise<Artifact>>();
const persistedImages = new Map<string, Artifact>();
const manualOperations: Operation[] = [];
let activeOperation: Operation | undefined;
let manualGeneration: Operation | undefined;
let downloadOperation: Operation | undefined;
let lastObservedState: Data | undefined;
let serial = Promise.resolve();
let confirmation: boolean | undefined;
let persistenceTimer: ReturnType<typeof setTimeout> | undefined;
let restoredApplied = false;
let restoredSettingsReady = false;
const pause = (ms = 35) => new Promise<void>((resolve) => setTimeout(resolve, ms));
const record = (value: unknown): Data => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Data : {};
const list = (value: unknown): Data[] => Array.isArray(value) ? value.map(record) : [];

/** Preserve the original UI diagnostic and propagate caught failures to the agent receipt. */
export function reportOperationError(error: unknown) {
  if (activeOperation) throw error instanceof Error ? error : new Error(String(error));
}

export function bindController(controller: Controller, name = "app") {
  controllers.set(name, controller);
  const release = () => { if (controllers.get(name) === controller) controllers.delete(name); };
  if (name !== "app" || !controller.state.storageHydrated) return release;
  for (const job of list(controller.state.codexJobs)) recoverableJobs.set(String(job.id), job);
  const previousState = lastObservedState ?? {...controller.state, codexJobs: [], animationTournamentMonitors: []};
  lastObservedState = controller.state;
  if (shouldTrackManualGeneration(controller.state, executionBaseline(previousState), Boolean(activeOperation), Boolean(manualGeneration))) {
    beginManualGeneration(previousState, true);
  }
  if (!restoredApplied) {
    restoredApplied = true;
    const restored = record(globalThis.__jarvisPixelRestored);
    const settings = record(restored.settings);
    void (async () => {
      for (const group of settingApplicationGroups(Object.entries(settings))) {
        for (const [key, value] of group) controller.setters[key]?.(value);
        await settle();
      }
      restoredSettingsReady = true;
    })();
  }
  if (persistenceTimer) clearTimeout(persistenceTimer);
  persistenceTimer = setTimeout(() => {
    void persistSession().catch((error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      if (activeOperation) activeOperation.error = `Não foi possível salvar a sessão: ${message}`;
      console.error("Pixel Cockpit session persistence failed:", message);
    });
  }, 400);
  return release;
}

function controller(name = "app") {
  const current = controllers.get(name);
  if (!current) throw new Error(`A área ${name} ainda não está aberta. Abra-a pelo comando correspondente antes de editar.`);
  return current;
}

export function getRecoverableJob(jobId: string) {
  return recoverableJobs.get(jobId);
}

function summarize(value: unknown): unknown {
  return JSON.parse(JSON.stringify(value, (_key, entry: unknown) => {
    if (typeof entry === "string" && (entry.startsWith("data:") || entry.startsWith("blob:"))) return { imageDataAvailable: true, characters: entry.length };
    if (entry instanceof Blob) return { blobAvailable: true, type: entry.type, size: entry.size };
    if (entry instanceof Map) return Object.fromEntries(entry);
    if (entry instanceof Set) return [...entry];
    return entry;
  }) ?? "null");
}

function hasReferenceShape(value: Data, required: string[], optional: string[] = []) {
  return required.every((key) => Object.hasOwn(value,key))
    && Object.keys(value).every((key) => required.includes(key) || optional.includes(key));
}

function resolveArgument(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(resolveArgument);
  const object = record(value);
  if (typeof object.saved === "string" && hasReferenceShape(object,["saved"])) {
    if (!values.has(object.saved)) throw new Error(`Resultado salvo inexistente: ${object.saved}`);
    return values.get(object.saved);
  }
  if (typeof object.state === "string" && hasReferenceShape(object,["state"],["controller"])
    && (!Object.hasOwn(object,"controller") || typeof object.controller === "string")) {
    const area = typeof object.controller === "string" ? object.controller : "app";
    return object.state.split(".").reduce<unknown>((current, key) => Array.isArray(current) ? current[Number(key)] : record(current)[key], controller(area).state);
  }
  if (typeof object.ref === "string" && typeof object.id === "string" && hasReferenceShape(object,["ref","id"])) {
    const result = list(controller().state[object.ref]).find((item) => item.id === object.id || item.name === object.id);
    if (!result) throw new Error(`Referência inexistente: ${object.ref}/${object.id}`);
    return result;
  }
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(object).map(([key, item]) => [key, resolveArgument(item)]));
  return value;
}

async function api(path: string, data?: unknown) {
  const response = await fetch(path, data === undefined ? undefined : {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(data),
  });
  const result: unknown = await response.json();
  if (!response.ok) throw new Error(String(record(result).error ?? `HTTP ${response.status}`));
  return result;
}

async function saveBlob(blob: Blob, name: string, owner = activeOperation): Promise<Artifact> {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  for (let index = 0; index < bytes.length; index += 32768) binary += String.fromCharCode(...bytes.subarray(index, index + 32768));
  const result = await api("/jarvis/artifacts", { name, mime: blob.type || "application/octet-stream", base64: btoa(binary) }) as Artifact;
  owner?.artifacts.push(result);
  return result;
}

globalThis.__jarvisPixelDownload = (blob: Blob, name: string) => {
  const owner = downloadOperation ?? activeOperation;
  const manual: Operation | undefined = owner ? undefined : {
    id: crypto.randomUUID().replaceAll("-", ""), action: "export", status: "running", phase: "Salvando arquivo",
    logs: [], artifacts: [], startedAt: new Date().toISOString(),
  };
  if (manual) manualOperations.push(manual);
  const promise = saveBlob(blob, name, owner);
  pendingDownloads.add(promise);
  void promise.then((artifact) => {
    pendingDownloads.delete(promise);
    if (manual) {
      manual.artifacts.push(artifact);
      manual.status = "completed";
      manual.phase = "Arquivo salvo";
      manual.finishedAt = new Date().toISOString();
    }
  }, (error: unknown) => {
    pendingDownloads.delete(promise);
    if (owner) {
      owner.error = error instanceof Error ? error.message : String(error);
      owner.status = "failed";
    }
    if (manual) { manual.status = "failed"; manual.phase = "Falha"; manual.error = String(error); manual.finishedAt = new Date().toISOString(); }
  });
};

const originalConfirm = window.confirm.bind(window);
window.confirm = (message: string) => {
  if (!activeOperation) return originalConfirm(message);
  if (confirmation === undefined) throw new Error(`Esta ação precisa de aprovação explícita: ${message}`);
  return confirmation;
};

async function settle() {
  await suiteTask();
  await Promise.all([...pendingDownloads]);
  await suiteTask();
}

// Timer throttling in hidden CEF views must not delay React state/effect synchronization.
function suiteTask() {
  return new Promise<void>((resolve) => {
    const channel = new MessageChannel();
    channel.port1.onmessage = () => {
      channel.port1.close();
      channel.port2.close();
      resolve();
    };
    channel.port2.postMessage(null);
  });
}

const persistedSettingNames = [
  "selectedId", "activeActionName", "selectedAnimationPresetId", "animationFrameCount",
  "animationBodyTopology", "animationMotionVariant", "recentAnimationPresetIds", "favoriteAnimationPresetIds",
  "animationPackExportDraft", "workflowMode", "providerId", "animationGenerationMode", "animationSourceId",
  "prompt", "negativePrompt", "jobNotes", "seed", "size", "count", "quality", "grid", "showGrid", "showCenter",
  "tool", "annotationColor", "annotationsByItem", "selectedFrameId", "chromaTolerance", "animationChromaKey",
  "animationDirectionPreset", "animationSingleDirection", "animationGenerationProfile", "motionPilotEnabled",
  "batchMatrixSourceIds", "batchMatrixPresetIds", "effectCategoryId", "effectTypeId", "effectStyleId",
  "effectPaletteId", "effectFrameCount", "effectCanvasSize", "effectLayoutId", "effectLoopMode",
  "effectAnchorMode", "effectBackgroundPreview", "language", "jobNotificationsEnabled",
];

const persistSession = createSnapshotWriter(() => {
  if (!controllers.has("app") || !controller().state.storageHydrated) return;
  const state = controller().state;
  return {
    version: 2,
    history: state.history, frames: state.frames, actions: state.actions,
    animationLibrary: state.userAnimationLibrary, pendingJobs: state.codexJobs,
    vfxComposite: localStorage.getItem("image-cockpit.vfx-composite.last"),
    settings: Object.fromEntries(persistedSettingNames.map((key) => [key, state[key]])),
    operations: [...operations.values()],
    operationJobs: Object.fromEntries([...operationJobs].map(([id, jobs]) => [id, [...jobs]])),
    recoverableJobs: [...recoverableJobs.values()],
  };
}, async (serialized) => { await api("/jarvis/state",JSON.parse(serialized)); });

async function persistFinalReceipt(operation: Operation) {
  try { await persistSession(); }
  catch (error) {
    operation.logs.push(`Não foi possível preservar o recibo para retomada: ${String(error)}`);
    if (operation.status === "completed") {
      operation.status = "blocked";
      operation.phase = "Resultado produzido; persistência pendente";
      operation.error = "Os arquivos foram produzidos, mas a sessão não confirmou sua persistência. Consulte os artefatos antes de repetir a operação.";
    }
  }
}

export async function restoreSession() {
  const restored = record(await api("/jarvis/state"));
  globalThis.__jarvisPixelRestored = restored;
  for (const job of [...list(restored.pendingJobs), ...list(restored.recoverableJobs)]) recoverableJobs.set(String(job.id), job);
  for (const [id, jobs] of Object.entries(record(restored.operationJobs))) {
    if (Array.isArray(jobs)) operationJobs.set(id, new Set(jobs.filter((job): job is string => typeof job === "string")));
  }
  for (const value of list(restored.operations)) {
    if (typeof value.id !== "string" || typeof value.action !== "string" || !Array.isArray(value.artifacts) || !Array.isArray(value.logs)) continue;
    const operation = value as unknown as Operation;
    if (operation.status === "running") {
      operation.status = "blocked";
      operation.phase = "Execução interrompida";
      operation.error = "A sessão foi interrompida antes da confirmação do resultado. Os arquivos foram preservados.";
      operation.nextAction = "Consulte originalJobIds e retome explicitamente o mesmo job; nenhuma geração será reiniciada pela consulta.";
      operation.recoveryRequired = true;
      operation.finishedAt = new Date().toISOString();
    }
    if (!operation.finishedAt) operation.finishedAt = new Date().toISOString();
    operation.originalJobIds = [...operationJobs.get(operation.id) ?? []];
    operations.set(operation.id, operation);
  }
  // Queued submissions are never automatically replayed after a host restart.
  localStorage.setItem("image-cockpit.pendingCodexJob", "[]");
  if (typeof restored.vfxComposite === "string") localStorage.setItem("image-cockpit.vfx-composite.last", restored.vfxComposite);
  localStorage.setItem("image-cockpit.language", String(record(restored.settings).language ?? "pt-BR"));
}

async function configure(settings: Data, area = "app") {
  const current = controller(area);
  const changes: Array<[string, {setter: Callable; value: unknown}]> = [];
  for (const [key, value] of Object.entries(settings)) {
    const setter = current.setters[key];
    if (!setter) throw new Error(`Parâmetro desconhecido em ${area}: ${key}`);
    let resolved = resolveArgument(value);
    if (["animationMotionVariant", "animationPackExportDraft"].includes(key)
      && resolved && typeof resolved === "object" && !Array.isArray(resolved)) resolved = {...record(current.state[key]), ...record(resolved)};
    if (area === "app") validateSetting(key, resolved, current.state[key]);
    else validateSuiteSetting(area, key, resolved, current.state);
    if (key === "effectCategoryId" && !list(current.state.effectCategories).some((category) => category.id === resolved)) throw new Error("Categoria de efeito desconhecida.");
    if (key === "effectTypeId") {
      const categoryId = settings.effectCategoryId ?? current.state.effectCategoryId;
      const category = list(current.state.effectCategories).find((entry) => entry.id === categoryId);
      if (!list(category?.types).some((entry) => entry.id === resolved)) throw new Error("Tipo de efeito não pertence à categoria selecionada.");
    }
    if (key === "animationMotionVariant") {
      for (const [modifier, option] of Object.entries(record(resolved))) {
        const allowed = record(current.state.motionVariantOptions)[modifier];
        if (!Array.isArray(allowed) || !allowed.includes(option)) throw new Error(`Modificador de movimento inválido: ${modifier}`);
      }
    }
    if (key === "batchMatrixSourceIds" && (resolved as string[]).some((id) => !list(current.state.history).some((entry) => entry.id === id))) throw new Error("A matriz contém uma imagem inexistente no histórico.");
    if (key === "batchMatrixPresetIds" && (resolved as string[]).some((id) => !list(current.state.animationPresets).some((entry) => entry.id === id))) throw new Error("A matriz contém uma receita inexistente.");
    changes.push([key, {setter, value: resolved}]);
  }
  if (area === "app") validateOriginalMotion(Object.fromEntries(changes.map(([key,value]) => [key,value.value])));
  for (const group of area === "app" ? settingApplicationGroups(changes) : [changes]) {
    for (const [, {setter, value}] of group) setter(value);
    await settle();
  }
  if (area === "app" && "animationFrameCount" in settings) current.methods.selectAnimationFrameCount(settings.animationFrameCount);
  if (area === "app" && "animationBodyTopology" in settings) current.methods.selectAnimationBodyTopology(settings.animationBodyTopology);
  await settle();
}

function validateOriginalMotion(changes: Data) {
  if (!["animationBodyTopology","animationFrameCount","animationMotionVariant"].some((key) => key in changes)) return;
  const state = controller().state;
  validateMotionConfiguration(state,changes);
  if (!state.selectedMotionRecipe) return;
  const candidate = {...state,...changes};
  const compile = record(record(modules).motionRecipes).compileMotionRecipe as Callable;
  try {
    compile({recipe:state.selectedMotionRecipe,directions:state.selectedAnimationDirections,topology:candidate.animationBodyTopology,
      frameCount:candidate.animationFrameCount,variant:candidate.animationMotionVariant});
  } catch (error) {
    throw new Error(`A combinação não é aceita pela receita selecionada; nenhuma configuração foi alterada. ${String(error)}`);
  }
}

async function resolveFile(args: Data) {
  const data = typeof args.path === "string" ? record(await api("/jarvis/import", { path: args.path })) : args;
  if (typeof data.dataUrl !== "string" || !data.dataUrl.startsWith("data:")) throw new Error("Informe path ou dataUrl para importar.");
  const response = await fetch(data.dataUrl);
  const blob = await response.blob();
  return new File([blob], String(data.name ?? "source.png"), { type: blob.type });
}

async function resolveParameter(value: unknown): Promise<unknown> {
  if (Array.isArray(value)) return Promise.all(value.map(resolveParameter));
  const object = record(value);
  if (object.file && typeof object.file === "object" && !Array.isArray(object.file) && hasReferenceShape(object,["file"])) return resolveFile(record(object.file));
  return resolveArgument(value);
}

async function importFile(args: Data, pack = false) {
  const file = await resolveFile(args);
  const historyBefore = list(controller().state.history).map((item) => item.id);
  const parsedPack = pack ? record(await (record(record(modules).animationPack).importAnimationPackBlob as Callable)(file, file.name)) : undefined;
  await controller().methods[pack ? "handleAnimationPackFiles" : "handleFiles"]([file]);
  await settle();
  if (pack && !list(controller().state.userAnimationLibrary).some((item) => item.title === parsedPack?.title)) throw new Error(String(controller().state.status ?? "O pacote não foi importado."));
  if (!pack && !list(controller().state.history).some((item) => !historyBefore.includes(item.id))) throw new Error(String(controller().state.status ?? "A imagem não foi importada."));
  if (pack) return summarize(list(controller().state.userAnimationLibrary).find((item) => item.title === parsedPack?.title));
  return summarize(controller().state.selected);
}

function updateLiveOperation(operation: Operation) {
  const state = controller().state;
  const jobs = list(state.codexJobs);
  const monitors = list(state.animationTournamentMonitors);
  const baseline = knownBeforeOperation.get(operation.id) ?? new Set<string>();
  const tracked = operationJobs.get(operation.id) ?? new Set<string>();
  for (const item of [...jobs, ...list(state.codexJobLogs)]) {
    const id = String(item.id ?? item.jobId);
    if (!baseline.has(id)) tracked.add(id);
  }
  operationJobs.set(operation.id, tracked);
  operation.originalJobIds = [...tracked];
  const candidates = monitors.flatMap((monitor) => list(record(monitor.manifest).candidates)).filter((candidate) => tracked.has(String(candidate.jobId)));
  if (candidates.length) {
    operation.progress = {
      completed: candidates.filter((candidate) => ["accepted", "failed", "cancelled", "quality-evaluated"].includes(String(candidate.state))).length,
      total: candidates.length,
      unit: "candidatos avaliados",
    };
  }
  operation.phase = state.isAnimationPreviewBuilding || state.isEffectPreviewBuilding
    ? "Preparando previews"
    : jobs.some((job) => job.state === "running") ? "Gerando e verificando os arquivos"
    : jobs.length ? "Aguardando capacidade do Codex"
    : state.isBusy ? "Preparando imagens e solicitação" : localizePixelText(String(state.status ?? "Processando"));
  operation.logs = list(state.codexJobLogs).filter((log) => tracked.has(String(log.jobId))).map((log) =>
    `[${log.jobId}] ${log.state}\n${typeof log.text === "string" ? log.text : ""}${log.error ? `\n${log.error}` : ""}`);
}

async function captureHistory(before: Set<unknown>, owner = activeOperation) {
  const generated = list(controller().state.history).filter((item) => !before.has(item.id));
  for (const item of generated) {
    if (typeof item.dataUrl !== "string") continue;
    const existing = persistedImages.get(String(item.id));
    if (existing) {
      owner?.artifacts.push(existing);
      continue;
    }
    const blob = await (await fetch(item.dataUrl)).blob();
    const artifact = await saveBlob(blob, String(item.name ?? `${item.id}.png`), owner);
    persistedImages.set(String(item.id), artifact);
  }
  return generated;
}

async function generate(args: Data, operation: Operation) {
  const mode = args.workflow ?? args.workflowMode ?? controller().state.workflowMode;
  if (!["image-generate", "image-edit", "sprite-generate", "effect-animation"].includes(String(mode))) throw new Error("Fluxo de geração inválido.");
  await controller().methods.beginWorkflow(mode);
  await settle();
  const provider = args.provider ?? "codex-handoff";
  if (!["codex-handoff", "local-generator"].includes(String(provider))) throw new Error("Para gerar use codex-handoff ou local-generator explícito; arquivos e caixa de entrada têm operações próprias.");
  await configure({ ...record(args.parameters), workflowMode: mode, providerId: provider });
  // A cancellation during configuration must never submit a later model request.
  if (operation.status !== "running" || cancellations.has(operation.id)) return;
  const baseline = executionBaseline(controller().state);
  dispatching.add(operation.id);
  try {
    await controller().methods.handleGenerate();
  } finally {
    dispatching.delete(operation.id);
  }
  await settle();
  await waitForGeneration(operation, baseline, String(mode));
}

async function waitForGeneration(operation: Operation, baseline: ExecutionBaseline, mode?: string) {
  let observedWork = false;
  while (operation.status === "running") {
    updateLiveOperation(operation);
    const state = controller().state;
    const observation = observeExecution(state, baseline);
    observedWork ||= observation.pending || observation.tournaments.length > 0;
    if (observation.pending) {
      await pause(1000);
      continue;
    }
    const fresh = await captureHistory(baseline.history, operation);
    if (observation.failures.length) {
      operation.status = "blocked";
      operation.phase = "Geração bloqueada; resultados preservados";
      const originalError = String(record(observation.failures[0].diagnostic).userMessage ?? observation.failures[0].message ?? state.status);
      const originalSuggestion = String(record(observation.failures[0].diagnostic).suggestion ?? "Consulte os logs e retome somente a etapa pendente.");
      operation.logs.push(originalError, originalSuggestion);
      operation.error = localizePixelText(originalError);
      operation.nextAction = localizePixelText(originalSuggestion);
      operation.result = summarize({ failures: observation.failures, preserved: fresh });
      break;
    }
    if (observation.stoppedTournaments.length) {
      const stopped = observation.stoppedTournaments.find((item) => item.outcome === "failed") ?? observation.stoppedTournaments[0];
      const reasons = list(stopped.manifest.candidates).filter((candidate) => candidate.state === "failed")
        .map((candidate) => String(candidate.reason ?? "")).filter(Boolean);
      operation.status = "blocked";
      operation.phase = stopped.outcome === "failed" ? "Bloqueado na verificação de qualidade" : "Aguardando revisão dos candidatos";
      operation.logs.push(...reasons);
      operation.error = stopped.outcome === "failed"
        ? `O candidato não passou na verificação de qualidade. ${reasons.map(localizePixelText).join("; ")} Nenhum executor deste torneio continua ativo.`
        : stopped.outcome === "cancelled" ? "Os candidatos foram cancelados; nenhum resultado foi aprovado."
        : "A geração terminou, mas os candidatos ainda precisam de revisão antes da publicação.";
      operation.nextAction = `Abra a revisão do torneio ${String(stopped.manifest.tournamentId)} e confira os motivos. Preserve os resultados aprovados e retome apenas a célula pendente após decidir a correção.`;
      operation.result = summarize({stoppedTournaments:observation.stoppedTournaments,preserved:fresh});
      break;
    }
    if (observation.repairReview) {
      const {manifest,candidate} = observation.repairReview;
      const directions = (candidate.repairDirections as string[]).map(localizePixelText).join(", ");
      operation.status = "blocked";
      operation.phase = "Reparo gerado; aguardando aceitação";
      operation.error = `O reparo de ${directions} foi gerado e está pronto para revisão. As direções não selecionadas continuam preservadas; a publicação aguarda a aceitação do reparo.`;
      operation.nextAction = `Abra review_open no torneio ${String(manifest.tournamentId)}. Após conferir o candidato ${String(candidate.jobId)}, use acceptDirectionRepairFromUi para aceitar o reparo existente, sem gerar outra imagem.`;
      operation.result = summarize({repairReview:observation.repairReview,preserved:fresh});
      break;
    }
    if (observation.review) {
      operation.status = "blocked";
      operation.phase = observation.review.state === "failed"
        ? "Bloqueado na verificação de qualidade" : "Aguardando revisão dos candidatos";
      operation.error = observation.review.state === "failed"
        ? `O torneio não publicou um candidato aprovado. ${localizePixelText(String(state.status ?? ""))}`
        : "Os candidatos precisam de uma decisão de revisão antes da publicação.";
      operation.nextAction = `Use review_open e os comandos de revisão no torneio ${observation.review.tournamentId}.`;
      operation.result = summarize({ review: observation.review, preserved: fresh });
      break;
    }
    if (fresh.length) {
      downloadOperation = operation;
      try {
        for (const item of fresh) {
          const effect = Boolean(item.effectAnimation) || mode === "effect-animation";
          const animation = !effect && (Array.isArray(item.animationDirections) || Boolean(item.animationPackV2) || mode === "sprite-generate");
          if (!effect && !animation) continue;
          await controller().methods.selectHistoryResult(item);
          await settle();
          const deadline = Date.now() + 60_000;
          while (controller().state.isAnimationPreviewBuilding || controller().state.isEffectPreviewBuilding) {
            if (Date.now() > deadline) throw new Error("O resultado foi preservado, mas a preparação do GIF não terminou. Consulte a prévia e exporte novamente sem gerar outra imagem.");
            await pause(100);
          }
          const before = operation.artifacts.length;
          await controller().methods[effect ? "exportSelectedEffectGif" : "exportDirectionalAnimations"]("gif");
          await settle();
          if (!operation.artifacts.slice(before).some((artifact) => artifact.mime === "image/gif")) {
            throw new Error("O resultado foi preservado, mas o exportador não produziu o GIF esperado. Revise a prévia e retome a exportação sem gerar outra imagem.");
          }
        }
      } finally {
        if (downloadOperation === operation) downloadOperation = undefined;
      }
      operation.result = summarize(fresh);
      break;
    }
    if (observedWork) {
      operation.status = "blocked";
      operation.error = `A execução terminou sem publicar uma imagem nova. ${localizePixelText(String(state.status ?? ""))}`;
      operation.nextAction = "Inspecione o torneio e seus candidatos; aceite o reparo ou escolha um candidato de revisão antes de exportar.";
      operation.result = summarize(observation.tournaments);
      break;
    }
    throw new Error(String(state.status ?? "A geração não foi iniciada."));
  }
}

async function attachOperationLogs(operation: Operation) {
  const jobIds = [...operationJobs.get(operation.id) ?? []];
  if (!jobIds.length) return;
  try {
    const logs = record(await api("/jarvis/logs", { jobIds }));
    operation.artifacts.push(...(Array.isArray(logs.artifacts) ? logs.artifacts as Artifact[] : []));
  } catch (error) { operation.logs.push(`Não foi possível anexar o log completo: ${String(error)}`); }
}

/** Track the existing UI job; this never starts or repeats a model request. */
export function beginManualGeneration(previousState = controller().state, observedJob = false) {
  if (activeOperation || manualGeneration) return;
  if (!observedJob && !["codex-handoff", "local-generator"].includes(String(controller().state.providerId))) return;
  const baseline = executionBaseline(previousState);
  const operation: Operation = {
    id: crypto.randomUUID().replaceAll("-", ""), action: "generate", status: "running",
    phase: "Preparando geração na suíte", logs: [], artifacts: [], startedAt: new Date().toISOString(),
  };
  manualGeneration = operation;
  operations.set(operation.id, operation);
  manualOperations.push(operation);
  knownBeforeOperation.set(operation.id, new Set([...list(previousState.codexJobs), ...list(previousState.codexJobLogs)].map((item) => String(item.id ?? item.jobId))));
  const mode = String(controller().state.workflowMode);
  void (async () => {
    try {
      await settle();
      await waitForGeneration(operation, baseline, mode);
      await cancellations.get(operation.id);
      await persistFinalReceipt(operation);
      if (operation.status === "running") {
        operation.status = "completed";
        operation.phase = "Concluído";
      }
    } catch (error) {
      await cancellations.get(operation.id);
      if (operation.status !== "cancelled") {
        operation.status = "failed";
        operation.phase = "Falha";
        operation.error = error instanceof Error ? error.message : String(error);
      }
    } finally {
      await attachOperationLogs(operation);
      operation.finishedAt = new Date().toISOString();
      if (manualGeneration === operation) manualGeneration = undefined;
      await persistFinalReceipt(operation);
    }
  })();
}

async function exportResult(args: Data) {
  const format = String(args.format ?? "image");
  const methods = controller().methods;
  if (format === "image") {
    const selected = record(controller().state.selected);
    if (typeof selected.dataUrl !== "string") throw new Error("Selecione uma imagem para exportar.");
    return saveBlob(await (await fetch(selected.dataUrl)).blob(), String(selected.name ?? "pixel-art.png"));
  }
  const mapping: Record<string, [string, unknown[]]> = {
    image: ["downloadSelectedImage", []],
    "direction-gif": ["exportDirectionalAnimations", ["gif"]],
    "direction-apng": ["exportDirectionalAnimations", ["apng"]],
    "direction-webp": ["exportDirectionalAnimations", ["webp"]],
    "animation-pack": ["exportSelectedAnimationPack", []],
    "effect-gif": ["exportSelectedEffectGif", []],
    "effect-apng": ["exportSelectedEffectApng", []],
    "effect-sheet": ["exportSelectedEffectSheet", []],
    "effect-frames": ["exportSelectedEffectFramesZip", []],
    "effect-metadata": ["exportSelectedEffectMetadata", []],
    "effect-pack": ["exportSelectedEffectPack", []],
  };
  const target = mapping[format];
  if (!target) throw new Error(`Formato desconhecido: ${format}`);
  const before = activeOperation?.artifacts.length ?? 0;
  await methods[target[0]](...target[1]);
  await settle();
  if ((activeOperation?.artifacts.length ?? 0) === before) throw new Error(`O resultado selecionado não oferece ${format}. ${String(controller().state.status ?? "")}`);
}

async function processImage(action: "recover_grid" | "extract_poses", args: Data, operation: Operation) {
  const state = controller().state;
  const source = selectProcessingSource(state, args);
  const selection = processingSelection(state);
  if (args.options !== undefined && (!args.options || typeof args.options !== "object" || Array.isArray(args.options))) throw new Error("options deve ser um objeto com os parâmetros do catálogo.");
  if (args.addToTimeline !== undefined && typeof args.addToTimeline !== "boolean") throw new Error("addToTimeline deve ser true ou false.");
  const options = action === "extract_poses"
    ? { columns:source.frameId ? 1 : record(state.grid).columns ?? 1, rows:source.frameId ? 1 : record(state.grid).rows ?? 1, ...record(args.options) }
    : record(args.options);
  operation.phase = action === "recover_grid" ? "Recuperando grade de pixels" : "Extraindo poses completas";
  operation.logs.push(operation.phase + " · processamento local, sem nova geração");
  const result = await processNativeImage(action, source.dataUrl, options);
  if (operation.status !== "running") return { cancelled:true, originalPreserved:true };
  const change = deriveProcessedAssets(controller().state, source, result, operation.id, args.addToTimeline === true || (action === "extract_poses" && args.addToTimeline !== false));
  for (let index=0; index<result.images.length; index++) {
    await saveBlob(await (await fetch(result.images[index].dataUrl)).blob(), change.history[index].name);
  }
  await saveBlob(new Blob([JSON.stringify({ ...result.report, sourceId:source.id, sourceFrameId:source.frameId, sourceName:source.name, options },null,2)],{type:"application/json"}),source.name.replace(/\.[^.]+$/,"") + "-" + action + ".json");
  if (operation.status !== "running") return { report:result.report, originalPreserved:true };
  const current = controller();
  current.setters.history((items:Data[]) => [...change.history,...items]);
  if (change.frames.length && change.action) {
    current.setters.frames((items:Data[]) => [...items,...change.frames.map((frame,index) => ({...frame,index:items.length+index}))]);
    current.setters.actions((items:Data[]) => [...items,change.action]);
  }
  // Do not undo a manual selection made while the native processor was running.
  if (processingSelection(current.state) === selection && record(current.state.selected).id === source.id && change.history[0]) {
    if (change.action) current.setters.activeActionName(change.action.name);
    current.setters.selectedId(change.history[0].id);
    current.setters.selectedFrameId("");
    if (action === "extract_poses") current.setters.grid({columns:result.report.columns,rows:result.report.rows,gutter:0});
  }
  current.setters.status("Processamento concluído; original preservado.");
  await settle();
  operation.logs.push(...(Array.isArray(result.report.warnings) ? result.report.warnings.map(String) : []));
  return { sourceId:source.id,sourceFrameId:source.frameId,report:result.report,historyIds:change.history.map((item) => item.id),frameIds:change.frames.map((item) => item.id),actionName:change.action?.name };
}

async function applyProductionRecipe(args: Data) {
  const recipe=productionRecipe(String(args.id ?? ""),String(args.direction ?? ""));
  const state=controller().state;
  const requestedSource=args.sourceId === undefined ? undefined : selectProcessingSource(state,args);
  if (recipe.kind === "animation") {
    const preset=list(state.animationPresets).find((item) => item.id === recipe.id);
    if (!preset) throw new Error("Receita original não encontrada no catálogo.");
    const module=record(record(modules).motionRecipes);
    const original=list(module.MOTION_RECIPES).find((item) => item.id === recipe.id);
    const preserved={animationBodyTopology:state.animationBodyTopology,animationFrameCount:state.animationFrameCount,animationMotionVariant:state.animationMotionVariant};
    const compile=module.compileMotionRecipe;
    if (typeof compile !== "function" || !original) throw new Error("Compilador de receitas indisponível.");
    // Validate the new recipe before its selector can reset manually configured parameters.
    compile({recipe:original,directions:state.selectedAnimationDirections,topology:preserved.animationBodyTopology,frameCount:preserved.animationFrameCount,variant:preserved.animationMotionVariant});
    await controller().methods.selectAnimationPreset(preset);
    await settle();
    await configure(preserved);
    await configure({workflowMode:"sprite-generate",...(requestedSource ? {selectedId:requestedSource.id,animationSourceId:requestedSource.id} : {})});
    if (!record(state.selected).id && !state.animationSourceId) controller().setters.status("Receita aplicada. Selecione uma referência para gerar a animação.");
  } else {
    const source=selectProcessingSource(state,args);
    await configure({workflowMode:"image-edit",selectedId:source.id,prompt:recipe.guidance});
  }
  await settle();
  return { ...recipe, applied:true, generated:false, originalPreserved:true };
}

async function execute(action: string, args: Data, operation: Operation): Promise<unknown> {
  switch (action) {
    case "catalog": return catalog(args);
    case "state": return summarize({
      ...controller(String(args.controller ?? "app")).state,
      interruptedOperations:[...operations.values()].filter((entry) => entry.recoveryRequired),
      recoverableJobs:[...recoverableJobs.values()],
    });
    case "configure": await configure(record(args.settings ?? args), String(args.controller ?? "app")); return summarize(controller(String(args.controller ?? "app")).state);
    case "recover_grid": return processImage("recover_grid",args,operation);
    case "extract_poses": return processImage("extract_poses",args,operation);
    case "production_recipe": return applyProductionRecipe(args);
    case "import_image": return importFile(args);
    case "import_animation_pack": return importFile(args, true);
    case "generate": await generate(args, operation); return operation.result;
    case "export": return exportResult(args);
    case "select": {
      const item = list(controller().state.history).find((entry) => entry.id === args.id);
      if (!item) throw new Error("Imagem não encontrada no histórico.");
      await controller().methods.selectHistoryResult(item);
      await settle();
      return summarize(item);
    }
    case "preset": {
      const kind = String(args.kind ?? "animation");
      const items = list(controller().state[kind === "prompt" ? "promptExamples" : "animationPresets"]);
      const item = items.find((entry) => entry.id === args.id);
      if (!item) throw new Error("Preset não encontrado. Consulte catalog({section:'options'}).");
      await controller().methods[kind === "prompt" ? "usePromptExample" : "useAnimationPresetExample"](item);
      await settle();
      return summarize(item);
    }
    case "annotate": {
      const rect = record(args.rect);
      const mode = String(args.tool ?? "rect");
      if (!["brush", "rect", "arrow"].includes(mode)) throw new Error("Ferramenta de anotação inválida: escolha brush, rect ou arrow.");
      let points = list(args.points);
      if (args.rect) {
        for (const key of ["x", "y", "width", "height"]) {
          if (typeof rect[key] !== "number" || !Number.isFinite(rect[key]) || rect[key] < 0 || rect[key] > 1) throw new Error("A região usa coordenadas normalizadas entre 0 e 1.");
        }
        if ((rect.x as number) + (rect.width as number) > 1 || (rect.y as number) + (rect.height as number) > 1 || !rect.width || !rect.height) throw new Error("A região deve estar dentro da imagem e ter área positiva.");
        points = [{x:rect.x,y:rect.y}, {x:(rect.x as number)+(rect.width as number),y:(rect.y as number)+(rect.height as number)}];
      }
      if (points.length < 2 || points.length > 10000 || points.some((point) => ["x","y"].some((axis) => typeof point[axis] !== "number" || !Number.isFinite(point[axis]) || point[axis] < 0 || point[axis] > 1))) throw new Error("Forneça de 2 a 10.000 pontos com x/y normalizados entre 0 e 1.");
      const annotation = await controller().methods.jarvisAddAnnotation(mode, points, String(args.comment ?? ""));
      await settle();
      return annotation;
    }
    case "timeline_open": await controller().methods.openAnimationTimelineEditor(); await settle(); return summarize(controller("timeline").state);
    case "vfx_open": await controller().methods.openVfxCompositeStage(); await settle(); return summarize(controller("vfx").state);
    case "review_open": {
      const manifest = args.manifest ?? await controller().methods.loadCodexAnimationTournament(args.tournamentId);
      await controller().methods.openAnimationReviewCockpit(manifest);
      await settle();
      return summarize(controller("review").state);
    }
    case "inspect": {
      const selected = record(controller().state.selected);
      if (typeof selected.dataUrl !== "string") throw new Error("Selecione uma imagem.");
      return saveBlob(await (await fetch(selected.dataUrl)).blob(), String(selected.name ?? "preview.png"));
    }
    case "invoke": {
      const method = String(args.method ?? "");
      const area = String(args.controller ?? "app");
      const member = typeof args.module === "string" ? record(record(modules)[args.module])[method] : controller(area).methods[method];
      if (member === undefined) throw new Error(`Operação inexistente: ${String(args.module ?? area)}.${method}`);
      const catalogRoot = catalogDefinition as unknown as Data;
      const methodSchema = typeof args.module === "string"
        ? record(record(record(catalogRoot.modules)[args.module])[method])
        : record(record(area === "app" ? catalogRoot.methods : record(record(catalogRoot.suites)[area]).methods)[method]);
      if (typeof args.module !== "string") validateCallable(method, String(methodSchema.parameters ?? ""), area);
      if (typeof args.module === "string" && !isPublicModuleMember(args.module, method, String(methodSchema.parameters ?? ""))) throw new Error("Esse helper depende de objetos internos do navegador; use import_image, inspect ou export.");
      if (args.module === "storage" && /^(save|clear)/.test(method)) throw new Error("Use a operação de histórico ou clearLocalStateScope com aprovação explícita.");
      if (["clearLocalStateScope", "resumeInterruptedCodexJob"].includes(method) && args.confirm !== true) throw new Error("Esta ação exige confirm=true após autorização explícita.");
      let parameters = Array.isArray(args.args) ? await Promise.all(args.args.map(resolveParameter)) : [];
      if (typeof args.module !== "string" && area === "app") {
        parameters = resolveInvocationArguments(method,parameters,controller().state);
        const state = controller().state;
        if (method === "selectAnimationBodyTopology") validateOriginalMotion({
          animationBodyTopology:parameters[0],
          animationMotionVariant:parameters[0] === "biped" ? state.animationMotionVariant : {...record(state.animationMotionVariant),weaponClass:"none",handedness:"inherit"},
        });
        if (method === "selectAnimationFrameCount") validateOriginalMotion({animationFrameCount:parameters[0]});
        if (method === "updateAnimationMotionVariant") {
          const [key,value] = parameters;
          const allowed = record(state.motionVariantOptions)[String(key)];
          if (!Array.isArray(allowed) || !allowed.includes(value)) throw new Error("Modificador ou valor de movimento inválido.");
          const variant = {...record(state.animationMotionVariant),[String(key)]:value};
          if (key === "weaponClass" && ["none","unarmed"].includes(String(value))) variant.handedness = "inherit";
          validateOriginalMotion({animationMotionVariant:variant});
        }
      }
      const artifactCount = operation.artifacts.length;
      const baseline = executionBaseline(controller().state);
      const recoveredJobId = method === "resumeInterruptedCodexJob" ? String(parameters[0]) : undefined;
      if (recoveredJobId) {
        baseline.failures.delete(recoveredJobId);
        baseline.jobs.delete(recoveredJobId);
        knownBeforeOperation.get(operation.id)?.delete(recoveredJobId);
        operationJobs.set(operation.id, new Set([recoveredJobId]));
      }
      const alreadyPending = observeExecution(controller().state, baseline).pending;
      if (operation.status !== "running" || cancellations.has(operation.id)) return;
      dispatching.add(operation.id);
      let result: unknown;
      try {
        result = typeof member === "function" ? await (member as Callable)(...parameters) : member;
      } finally {
        dispatching.delete(operation.id);
      }
      if (typeof args.saveAs === "string") values.set(args.saveAs, result);
      if (result instanceof Blob) result = await saveBlob(result, String(args.filename ?? `${method}.${result.type === "image/gif" ? "gif" : result.type === "application/zip" ? "zip" : "png"}`));
      await settle();
      const launchesGeneration = [
        "runAnimationBatchMatrix", "startPersistedTournamentInitialCandidatesFromUi",
        "startPersistedTournamentCandidate", "startDirectionRepairFromUi",
        "repairAnimationReviewDirection", "fallbackMotionPilotFromUi",
        "resumeInterruptedCodexJob",
      ].includes(method) || (area === "review" && method === "repairDirection");
      if (launchesGeneration || (!alreadyPending && observeExecution(controller().state, baseline).generationQueued)) {
        await waitForGeneration(operation, baseline);
        if (recoveredJobId && operation.status === "running") {
          // Only the exact, durably linked single-job operation can be reconciled here.
          for (const previous of operations.values()) {
            const jobs = operationJobs.get(previous.id);
            if (previous.id === operation.id || !jobs?.has(recoveredJobId) || jobs.size !== 1 || previous.status !== "blocked") continue;
            previous.status = "completed";
            previous.phase = "Recuperado e verificado";
            previous.artifacts = [...previous.artifacts, ...operation.artifacts];
            previous.result = operation.result;
            previous.logs = [...previous.logs, ...operation.logs];
            previous.finishedAt = new Date().toISOString();
            previous.recoveryRequired = false;
            delete previous.error;
            delete previous.nextAction;
          }
        }
        return operation.result;
      }
      if (/^(export|download)/.test(method) && operation.artifacts.length === artifactCount && result === undefined) throw new Error(`A operação ${method} não produziu um arquivo. ${String(controller(area).state.status ?? controller(area).state.message ?? "")}`);
      return summarize(result);
    }
    default: throw new Error(`Ação Pixel Art desconhecida: ${action}`);
  }
}

const actionDefinitions = {
  recover_grid: "Recupera a grade nativa localmente, sem IA e preservando o original. {sourceId? OU frameId?,addToTimeline?:false,options?:{cellWidth?:1..128,cellHeight?:1..128,phaseX?:0,phaseY?:0,paletteColors?:2..256,scale?:1..16,alphaThreshold?:1..255,chromaKey?:[r,g,b],tolerance?:0..120}}. Sem cellWidth/cellHeight detecta periodicidade; se ambígua, informe ambos. Gera PNGs derivados e relatório JSON.",
  extract_poses: "Extrai componentes completos antes de agrupar as poses; preserva partes desconectadas e pixels que atravessam células. {sourceId? OU frameId?,addToTimeline?:true,options?:{columns?:1..32,rows?:1..32,poseCells?:number[],padding?:0..64,minArea?:1..10000,alphaThreshold?:1..255,chromaKey?:[r,g,b],tolerance?:0..120,alignment?:'preserve'|'bottom'}}. Até 64 células. poseCells indica as células ocupadas em ordem de leitura, começando em zero, quando fragmentos ocupam células vazias. preserve mantém deslocamentos de salto; bottom alinha explicitamente. Acrescenta uma ação separada sem reescrever quadros, FPS ou ações anteriores.",
  production_recipe: "Aplica uma receita de produção sem gerar imagens. {id,direction?,sourceId?}; opções em catalog({section:'options'}). Referências neutras/por direção usam image-edit; animações usam receitas originais com disciplina adicional no compilador, preservando parâmetros compatíveis. Não troca provedor, modelo ou conta.",
  catalog: "Lista todas as operações originais e parâmetros. Filtros opcionais: query, module, controller.",
  state: "Lê o estado compartilhado com a suíte. controller opcional: app, timeline, review, vfx.",
  configure: "Define parâmetros originais: {settings:{nome:valor},controller?:string}. Consulte catalog; preserva o restante.",
  import_image: "Importa imagem real por {path} relativo/absoluto dentro do projeto ou {dataUrl,name}.",
  import_animation_pack: "Importa pacote ZIP/JSON original por {path} ou {dataUrl,name}.",
  generate: "Executa Codex real com {workflow,parameters}. Fluxos: image-generate, image-edit, sprite-generate, effect-animation. Preserva QA, torneios e publicação originais. provider:'local-generator' ativa somente o gerador procedural legado quando explicitamente solicitado; não é evidência de qualidade de geração por IA.",
  export: "Exporta resultado selecionado. {format}: image,direction-gif,direction-apng,direction-webp,animation-pack,effect-gif,effect-apng,effect-sheet,effect-frames,effect-metadata,effect-pack.",
  select: "Seleciona resultado do histórico por {id}.",
  preset: "Aplica preset original por {kind:'animation'|'prompt',id}; opções em catalog({section:'options'}).",
  annotate: "Anota a imagem por {tool:'rect'|'brush'|'arrow',points:[{x,y},...],comment} ou {rect:{x,y,width,height},comment}; coordenadas normalizadas 0..1 da imagem original.",
  timeline_open: "Abre editor de linha do tempo original para resultado selecionado; invoke controller=timeline para todas as operações.",
  review_open: "Abre revisão original por {tournamentId} ou {manifest}; invoke controller=review.",
  vfx_open: "Abre composição original de personagem+efeito; invoke controller=vfx.",
  inspect: "Retorna arquivo de preview da imagem selecionada.",
  invoke: "Operação original avançada {method,args:[],controller?} ou {module,method,args:[]}. Referências: {state:'campo.caminho',controller?}, {ref:'history'|'frames'|'actions',id}, {saved:'nome'}, {file:{path}}. Somente esses formatos exatos são referências; registros com outros campos são preservados. saveAs guarda resultado. invoke({controller:'timeline'|'review'|'vfx',method:'close'}) fecha a área. confirm=true somente com autorização explícita.",
};

function catalog(filter: Data = {}) {
  const raw = catalogDefinition as unknown as Data;
  const definition = {
    ...raw,
    modules: Object.fromEntries(Object.entries(record(raw.modules)).map(([name, schema]) => [name,
      Object.fromEntries(Object.entries(record(schema)).filter(([method, value]) =>
        isPublicModuleMember(name, method, String(record(value).parameters ?? ""))))])),
    suites: Object.fromEntries(Object.entries(record(raw.suites)).map(([name, schema]) => [name, {
      ...record(schema),
      settings: Object.fromEntries(Object.entries(record(record(schema).settings)).filter(([key]) => suiteSettingNames[name]?.has(key))),
    }])),
    methods: Object.fromEntries(Object.entries(record(raw.methods)).filter(([name, schema]) =>
      publicAppMethods.has(name) && !/React\.(?:Pointer|Mouse|UI)Event|HTMLElement|HTMLCanvasElement|=>|FileList/.test(String(record(schema).parameters ?? "")))),
    settings: Object.fromEntries(Object.entries(record(raw.settings)).filter(([name]) => configurableNames.has(name))),
  } as Data;
  if (filter.section === "options") {
    const state = controller().state;
    return summarize({ ...Object.fromEntries(["animationPresets", "promptExamples", "motionVariantOptions", "effectCategories", "effectStyles", "effectPalettes", "effectFrameCounts", "effectCanvasSizes"].map((key) => [key, state[key]])), productionRecipes });
  }
  if (filter.section === "types") {
    const types = record(definition.types);
    const query = String(filter.query ?? "").toLowerCase();
    return query
      ? {types:Object.fromEntries(Object.entries(types).filter(([name]) => name.toLowerCase().includes(query)))}
      : {types:Object.keys(types),details:"catalog({section:'types',query:'NomeDoTipo'})"};
  }
  if (typeof filter.method === "string") {
    return selectCatalogMethod(definition, filter);
  }
  if (typeof filter.section === "string") return { section: filter.section, value: definition[filter.section] };
  if (typeof filter.module === "string") return { module: filter.module, operations: record(definition.modules)[filter.module] };
  if (typeof filter.controller === "string" && filter.controller !== "app") return { controller: filter.controller, ...record(record(definition.suites)[filter.controller]) };
  if (typeof filter.query === "string" && filter.query) {
    const query = filter.query.toLowerCase();
    return {
      actions: Object.fromEntries(Object.entries(actionDefinitions).filter(([name, description]) => `${name} ${description}`.toLowerCase().includes(query))),
      methods: Object.fromEntries(Object.entries(record(definition.methods)).filter(([name, schema]) => `${name} ${JSON.stringify(schema)}`.toLowerCase().includes(query))),
      settings: Object.fromEntries(Object.entries(record(definition.settings)).filter(([name, schema]) => `${name} ${JSON.stringify(schema)}`.toLowerCase().includes(query))),
      modules: Object.keys(modules).filter((name) => name.toLowerCase().includes(query)),
    };
  }
  return {
    version: 1, actions: actionDefinitions,
    methods: Object.keys(record(definition.methods)), settings: Object.keys(record(definition.settings)),
    modules: Object.fromEntries(Object.entries(record(definition.modules)).map(([name, exports]) => [name, Object.keys(record(exports))])),
    suites: Object.fromEntries(Object.entries(record(definition.suites)).map(([name, schema]) => [name, {
      methods: Object.keys(record(record(schema).methods)), settings: Object.keys(record(record(schema).settings)),
    }])),
    details: "catalog({method}), catalog({controller,method}), catalog({module,method}), catalog({query}), catalog({section:'settings'}), catalog({section:'types',query:'NomeDoTipo'})",
    stateReferences: "Use state/ref/saved references to preserve original objects and image bytes without copying base64 into model context.",
  };
}

function start(request: { id: string; action: string; args?: Data }) {
  if (!request || typeof request.id !== "string" || !request.id || typeof request.action !== "string") throw new Error("Comando Pixel Art inválido.");
  if (operations.has(request.id)) return summarize(operations.get(request.id));
  const operation: Operation = { id: request.id, action: request.action, status: "running", phase: "Na fila", logs: [], artifacts: [], startedAt: new Date().toISOString() };
  operations.set(request.id, operation);
  serial = serial.then(async () => {
    if (operation.status === "cancelled") return;
    activeOperation = operation;
    confirmation = typeof request.args?.confirm === "boolean" ? request.args.confirm : undefined;
    try {
      const state = controller().state;
      const jobs = list(state.codexJobs);
      const readOnly = ["catalog", "state", "inspect"].includes(request.action);
      if ((state.isBusy || jobs.length > 0 || manualGeneration) && !readOnly
        && !(request.action === "invoke" && ["cancelCodexJob","cancelTournamentFromUi","loadCodexAnimationTournament"].includes(String(request.args?.method)))) {
        operation.status = "blocked";
        operation.error = "Há uma geração em andamento na suíte. Aguarde o resultado ou cancele essa execução antes de alterar seus parâmetros.";
        operation.nextAction = "Use state para acompanhar a geração e os identificadores dos jobs; cancelCodexJob cancela um job explicitamente.";
        return;
      }
      knownBeforeOperation.set(operation.id, new Set([...jobs, ...list(state.codexJobLogs)].map((item) => String(item.id ?? item.jobId))));
      operation.phase = "Executando";
      operation.result = await execute(request.action, request.args ?? {}, operation);
      await cancellations.get(operation.id);
      await settle();
      await persistFinalReceipt(operation);
      if (operation.status === "running") {
        operation.status = "completed";
        operation.phase = "Concluído";
      }
    } catch (error) {
      await cancellations.get(operation.id);
      if (operation.status !== "cancelled") {
        operation.status = "failed";
        operation.phase = "Falha";
        operation.error = error instanceof Error ? error.message : String(error);
      }
    } finally {
      await attachOperationLogs(operation);
      operation.finishedAt = new Date().toISOString();
      activeOperation = undefined;
      confirmation = undefined;
      await persistFinalReceipt(operation);
    }
  });
  return summarize(operation);
}

export function pixelProcessingActivity() {
  const operation=[...operations.values()].filter((item) => ["recover_grid","extract_poses","production_recipe"].includes(item.action)).at(-1);
  return operation ? operationReceipt(operation) : undefined;
}

export async function runSuiteOperation(action: string, args: Data) {
  if (!["recover_grid","extract_poses","production_recipe"].includes(action)) throw new Error("Operação de preparação inválida.");
  const id=crypto.randomUUID().replaceAll("-","");
  start({id,action,args});
  const operation=operations.get(id)!;
  manualOperations.push(operation);
  while (!operation.finishedAt) await pause(80);
  if (operation.status !== "completed") throw new Error(operation.error ?? operation.phase);
  return operation.result;
}

async function cancel(id: string) {
  if (cancellations.has(id)) return cancellations.get(id);
  const operation = operations.get(id);
  if (!operation || operation.status !== "running") return;
  if (operation !== activeOperation && operation !== manualGeneration) {
    operation.status = "cancelled";
    operation.phase = "Cancelado antes de iniciar";
    operation.finishedAt = new Date().toISOString();
    return;
  }
  operation.phase = "Cancelando";
  const cancellation = (async () => {
    try {
      const deadline = Date.now() + 90_000;
      const requested = new Set<string>();
      // Original submission may still be preparing references before the job appears.
      // Keep observing until that submission settles and every owned job is removed.
      while (true) {
        updateLiveOperation(operation);
        const jobs = list(controller().state.codexJobs).filter((job) => operationJobs.get(id)?.has(String(job.id)));
        for (const job of jobs) {
          if (requested.has(String(job.id))) continue;
          const response = record(await controller().methods.cancelCodexJob(job.id));
          if (response.ok === false) throw new Error(String(response.message ?? "O backend não confirmou o cancelamento."));
          requested.add(String(job.id));
        }
        if (!dispatching.has(id) && jobs.length === 0 && !(operation === manualGeneration && controller().state.isBusy)) break;
        if (Date.now() >= deadline) throw new Error("O cancelamento foi solicitado, mas o executor ainda não confirmou o encerramento. Consulte o estado do job antes de iniciar outra geração.");
        operation.phase = "Aguardando confirmação do cancelamento";
        await pause(100);
      }
      operation.status = "cancelled";
      operation.phase = "Cancelado";
      if (operation !== activeOperation && operation !== manualGeneration) operation.finishedAt = new Date().toISOString();
    } catch (error) {
      operation.status = "failed";
      operation.phase = "Não foi possível confirmar o cancelamento";
      operation.error = error instanceof Error ? error.message : String(error);
    }
  })();
  cancellations.set(id, cancellation);
  await cancellation;
}

globalThis.__jarvisPixel = {
  start,
  snapshot: (id?: string) => {
    if (!id) return { ready: Boolean(controllers.get("app")?.state.storageHydrated) && restoredSettingsReady, activeId: activeOperation?.id ?? manualGeneration?.id, controllers: [...controllers.keys()] };
    const operation = operations.get(id);
    return operation ? operationReceipt(operation) : null;
  },
  catalog,
  activity: () => manualOperations.map(operationReceipt),
  cancel: (id: string) => { void cancel(id); return { id, requested: true }; },
};

function operationReceipt(operation: Operation) {
  return summarize(operation.status !== "running" && !operation.finishedAt
    ? {...operation, status: "running", phase: "Finalizando arquivos e logs"}
    : operation);
}

declare global {
  var __jarvisPixelDownload: ((blob: Blob, name: string) => void) | undefined;
  var __jarvisPixelRestored: unknown;
  var __jarvisPixel: {
    start: typeof start;
    snapshot: (id?: string) => unknown;
    catalog: typeof catalog;
    activity: () => unknown;
    cancel: (id: string) => { id: string; requested: boolean };
  };
}
