export const publicAppMethods = new Set([
  "resumeInterruptedCodexJob",
  "exportEnvironmentReport",
  "usePromptExample", "openVfxSourceResult",
  "closeAnimationReviewCockpit", "closeVfxCompositeStage",
  "runAnimationBatchMatrix", "loadCodexAnimationTournament", "openAnimationReviewCockpit",
  "saveAnimationHumanReview", "adoptAnimationReviewWinner", "fallbackMotionPilotFromUi",
  "repairAnimationReviewDirection", "startPersistedTournamentInitialCandidatesFromUi",
  "startPersistedTournamentCandidate", "cancelTournamentFromUi", "toggleDirectionRepairSelection",
  "startDirectionRepairFromUi", "acceptDirectionRepairFromUi", "revalidateAcceptedPilotFromUi",
  "toggleBatchMatrixSource", "toggleBatchMatrixPreset", "cancelCodexJob",
  "retryCodexFailureImport", "dedupeLocalInboxHistoryNow", "dedupePersistedHistoryNow",
  "loadOutboxResults", "recoverUnimportedOutboxResults", "runCockpitHealthCheck", "repairCockpit",
  "importLatestOutboxResult", "splitSelectedToTimeline", "exportDirectionalAnimations",
  "exportSelectedEffectGif", "exportSelectedEffectApng", "exportSelectedEffectSheet",
  "exportSelectedEffectFramesZip", "exportSelectedEffectMetadata", "exportSelectedEffectPack",
  "downloadSelectedImage", "addSelectedAsFrame", "moveFrame", "removeFrame", "selectHistoryResult",
  "selectSourceFromPreview", "applyChromaToSelectedFrame", "updateActiveAction", "updateCell",
  "updateAnnotationComment", "removeAnnotation", "clearSelectedAnnotations", "adoptSelected",
  "beginWorkflow", "useQuickPrompt", "selectAnimationPreset", "toggleFavoriteAnimationPreset",
  "selectAnimationBodyTopology", "selectAnimationFrameCount", "updateAnimationMotionVariant",
  "useAnimationPresetExample", "useAnimationLibraryItem", "exportAnimationLibraryItem",
  "deleteUserAnimationItem", "renameUserAnimationItem", "buildSelectedAnimationPackV2", "openAnimationTimelineEditor",
  "saveAnimationTimelinePack", "exportSelectedAnimationPackV2", "exportAnimationEngineMetadata",
  "verifyAnimationPackV2RoundTrip", "exportSelectedAnimationPack", "openVfxCompositeStage",
  "clearLocalStateScope",
]);

export function isPublicModuleMember(module: string, method: string, parameters: string) {
  return !(module === "image" && ["createId", "loadImage", "canvasToBlob"].includes(method))
    && !(module === "storage" && /^(save|clear)/.test(method))
    && !/HTML(?:Canvas|Image|Video|Element)|(?:Pointer|Mouse|UI)Event|=>/.test(parameters);
}

export function selectCatalogMethod(definition: Record<string, unknown>, filter: Record<string, unknown>) {
  const record = (value: unknown): Record<string, unknown> => value && typeof value === "object" ? value as Record<string, unknown> : {};
  const method = String(filter.method);
  const selected = typeof filter.controller === "string" && filter.controller !== "app"
    ? record(record(definition.suites)[filter.controller]) : definition;
  const schema = typeof filter.module === "string"
    ? record(record(definition.modules)[filter.module])[method]
    : record(selected.methods)[method] ?? record(selected.settings)[method];
  const names = new Set(JSON.stringify(schema ?? "").match(/[A-Za-z_$][\w$]*/g) ?? []);
  return { method, module:filter.module, schema,
    relatedTypes:Object.fromEntries(Object.entries(record(definition.types)).filter(([name]) => names.has(name))),
  };
}

/** Original effect selectors reset preset defaults in React effects before custom values apply. */
export function settingApplicationGroups<T>(changes: Array<[string, T]>): Array<Array<[string, T]>> {
  const selectors = new Set(["effectCategoryId", "effectTypeId"]);
  const preset = changes.filter(([name]) => selectors.has(name));
  const overrides = changes.filter(([name]) => !selectors.has(name));
  return preset.length ? [preset, overrides].filter((group) => group.length) : [changes];
}

export const suiteSettingNames: Record<string, Set<string>> = {
  timeline: new Set(["direction", "frameIndex", "playing", "speed"]),
  review: new Set(["directionIndex", "frameIndex", "playing", "speed", "normalized", "sourceOpacity",
    "activeCandidateId", "heatMode", "showOnion", "showDiff", "showGround", "showPivot", "showSocket",
    "showHitbox", "showHurtbox", "background", "reducedMotion"]),
  vfx: new Set(["frameIndex", "playing"]),
};

export function validateSuiteSetting(area: string, name: string, value: unknown, state: Record<string, unknown>) {
  if (!suiteSettingNames[area]?.has(name)) throw new Error(`${area}.${name} é estado interno; use os métodos de edição.`);
  if (typeof value !== typeof state[name]) throw new Error(`${area}.${name}: tipo de valor inválido.`);
  const draft = state.draft as { directions?: string[]; frameOrder?: number[] } | undefined;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error(`${name}: informe um número finito.`);
    if (name === "speed" && ![0.5, 1, 1.5, 2].includes(value)) throw new Error("speed: escolha 0.5, 1, 1.5 ou 2.");
    if (name === "sourceOpacity" && (value < 0 || value > 1)) throw new Error("sourceOpacity deve estar entre 0 e 1.");
    if (name === "frameIndex" || name === "directionIndex") {
      const count = name === "directionIndex" ? (state.directions as string[] | undefined)?.length
        : draft?.frameOrder?.length ?? Number(state.frameCount);
      if (!Number.isInteger(value) || value < 0 || !count || value >= count) throw new Error(`${name}: índice fora dos quadros/direções disponíveis.`);
    }
  }
  if (name === "direction" && !draft?.directions?.includes(String(value))) throw new Error("Direção não existe no pacote selecionado.");
  if (name === "background" && !["checker", "light", "dark"].includes(String(value))) throw new Error("background: escolha checker, light ou dark.");
  if (name === "heatMode" && !["foot-drift", "bbox", "identity", "loop-seam", "static-copy", "clipping"].includes(String(value))) throw new Error("heatMode inválido.");
  if (name === "activeCandidateId" && !(state.candidates as { jobId: string }[] | undefined)?.some((entry) => entry.jobId === value)) throw new Error("Candidato inexistente.");
}

export const configurableNames = new Set([
  "selectedId", "activeActionName", "animationFrameCount", "animationBodyTopology",
  "animationMotionVariant", "animationPackExportDraft", "workflowMode", "providerId",
  "animationSourceId", "prompt", "negativePrompt", "jobNotes", "seed", "size", "count", "quality",
  "grid", "showGrid", "showCenter", "tool", "annotationColor", "selectedFrameId", "chromaTolerance",
  "animationChromaKey", "animationDirectionPreset", "animationSingleDirection",
  "animationGenerationProfile", "motionPilotEnabled", "batchMatrixSourceIds", "batchMatrixPresetIds",
  "effectCategoryId", "effectTypeId", "effectStyleId", "effectPaletteId", "effectFrameCount",
  "effectCanvasSize", "effectLayoutId", "effectLoopMode", "effectAnchorMode", "effectBackgroundPreview",
  "language", "jobNotificationsEnabled",
]);

const choices: Record<string, readonly unknown[]> = {
  workflowMode: ["image-generate", "image-edit", "sprite-generate", "effect-animation"],
  providerId: ["codex-handoff", "local-file", "local-inbox", "local-generator"],
  tool: ["select", "brush", "rect", "arrow"],
  language: ["ja", "en", "zh-CN", "zh-TW", "ko", "ru", "es", "pt-BR", "de", "fr", "id", "tr", "vi", "pl", "it"],
  animationFrameCount: [4, 6, 8, 12, 16, 20],
  animationChromaKey: ["green", "magenta"],
  animationDirectionPreset: ["one", "three", "five"],
  animationSingleDirection: ["front", "front three-quarter", "side", "back three-quarter", "back"],
  animationGenerationProfile: ["fast", "balanced", "best"],
  animationBodyTopology: ["biped", "quadruped", "serpentine-or-body-contact", "floating", "winged-flying", "multi-leg"],
  quality: ["auto", "low", "medium", "high"],
  effectStyleId: ["pixel-clean", "painterly-soft", "arcade-bold", "anime-flare"],
  effectPaletteId: ["cyan-white", "violet-gold", "ember-orange", "toxic-green", "mono-white"],
  effectLayoutId: ["grid-4x2", "horizontal-strip", "grid-4x4"],
  effectLoopMode: ["one-shot", "loop", "ping-pong-loop"],
  effectFrameCount: [4, 6, 8, 12, 16],
  effectCanvasSize: [64, 96, 128, 192, 256],
  effectAnchorMode: ["center", "bottom-center", "caster-hand", "projectile-center", "impact-ground"],
  effectBackgroundPreview: ["checkerboard", "dark", "light", "neutral"],
};

export function validateSetting(name: string, value: unknown, current: unknown, allowedValues?: readonly unknown[]) {
  if (!configurableNames.has(name)) throw new Error(`O campo ${name} é estado interno; use a operação correspondente.`);
  const values = allowedValues?.length ? allowedValues : choices[name];
  if (values && !values.includes(value)) throw new Error(`${name}: escolha ${values.join(", ")}.`);
  if (current !== undefined && current !== null && typeof value !== typeof current) throw new Error(`${name}: tipo de valor inválido.`);
  if (current && typeof current === "object" && !Array.isArray(current)
    && (!value || typeof value !== "object" || Array.isArray(value))) throw new Error(`${name}: informe um objeto.`);
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error(`${name}: informe um número finito.`);
    if (!Number.isInteger(value)) throw new Error(`${name}: informe um número inteiro.`);
    if (name === "count" && (value < 1 || value > 4)) throw new Error("count deve estar entre 1 e 4.");
    if (name === "chromaTolerance" && (value < 0 || value > 120)) throw new Error("chromaTolerance deve estar entre 0 e 120.");
    if (name === "effectFrameCount" && (value < 1 || value > 60)) throw new Error("effectFrameCount deve estar entre 1 e 60.");
    if (name === "effectCanvasSize" && value < 1) throw new Error("effectCanvasSize deve ser positivo.");
  }
  if (name === "grid") {
    const grid = value as Record<string, unknown>;
    for (const key of ["columns", "rows", "gutter"]) {
      const number = grid?.[key];
      if (typeof number !== "number" || !Number.isInteger(number) || number < (key === "gutter" ? 0 : 1)) throw new Error(`grid.${key} inválido.`);
    }
  }
  if (Array.isArray(current) && !Array.isArray(value)) throw new Error(`${name}: informe uma lista.`);
  if (name === "batchMatrixSourceIds" || name === "batchMatrixPresetIds") {
    if (!Array.isArray(value) || value.length > 4 || value.some((entry) => typeof entry !== "string") || new Set(value).size !== value.length) throw new Error(`${name}: informe até quatro identificadores distintos.`);
  }
  if (name === "animationPackExportDraft") {
    const draft = value as Record<string, unknown>;
    if (Object.keys(draft).some((key) => !["title", "tags", "license", "sourceNote", "promptSummary", "includePromptSummary"].includes(key))) throw new Error("Campo desconhecido nos metadados do pacote.");
    if (["title", "tags", "license", "sourceNote", "promptSummary"].some((key) => typeof draft[key] !== "string")
      || typeof draft.includePromptSummary !== "boolean") throw new Error("Metadados do pacote inválidos.");
  }
  if (name === "size" && (typeof value !== "string" || !/^[1-9]\d{0,4}x[1-9]\d{0,4}$/.test(value))) throw new Error("size deve informar largura x altura positivas, como 1024x1024.");
  if (typeof value === "string" && ["annotationColor"].includes(name) && !/^#[0-9a-f]{6}$/i.test(value)) throw new Error("Informe uma cor hexadecimal #RRGGBB.");
}

export function validateCallable(method: string, parameters: string, area = "app") {
  if (area === "app" && !publicAppMethods.has(method)) throw new Error(`${method} é um helper interno. Consulte catalog para a operação pública correspondente.`);
  if (/React\.(?:Pointer|Mouse|UI)Event|HTMLElement|HTMLCanvasElement|=>|FileList/.test(parameters)) throw new Error(`${method} precisa de uma interação de interface; use o adaptador de importação, anotação ou edição correspondente.`);
}

/** Match choices filtered by the original UI before calling React state setters. */
export function validateMotionConfiguration(state: Record<string, unknown>, changes: Record<string, unknown>) {
  const recipe = state.selectedMotionRecipe as {id?:string;supportedTopologies?:string[];allowedFrameCounts?:number[]};
  if (!recipe) return;
  const topology = changes.animationBodyTopology ?? state.animationBodyTopology;
  const frameCount = changes.animationFrameCount ?? state.animationFrameCount;
  if (!recipe.supportedTopologies?.includes(String(topology))) {
    throw new Error(`A receita ${recipe.id} não aceita a topologia ${topology}. Escolha ${recipe.supportedTopologies?.join(", ")} ou selecione outra receita.`);
  }
  if (!recipe.allowedFrameCounts?.includes(Number(frameCount))) {
    throw new Error(`A receita ${recipe.id} não aceita ${frameCount} quadros. Escolha ${recipe.allowedFrameCounts?.join(", ")}.`);
  }
}

export function resolveInvocationArguments(method: string, args: unknown[], state: Record<string, unknown>) {
  const catalogs: Record<string, string> = {
    usePromptExample:"promptExamples",useAnimationPresetExample:"animationPresets",selectAnimationPreset:"animationPresets",
    selectHistoryResult:"history",selectSourceFromPreview:"history",
  };
  const catalog = catalogs[method];
  if (catalog) {
    const id = args[0] && typeof args[0] === "object" ? (args[0] as {id?:unknown}).id : undefined;
    const match = (state[catalog] as {id:string}[] | undefined)?.find((entry) => entry.id === id);
    if (!match) throw new Error(`${method}: use um item existente de ${catalog}, consultado pelo catálogo ou pelo estado.`);
    return [match, ...args.slice(1)];
  }
  if (method === "toggleFavoriteAnimationPreset" &&
    !(state.animationPresets as {id:string}[] | undefined)?.some((entry) => entry.id === args[0])) {
    throw new Error("Preset de animação inexistente.");
  }
  if (method === "selectAnimationBodyTopology") {
    validateSetting("animationBodyTopology",args[0],state.animationBodyTopology);
    validateMotionConfiguration(state,{animationBodyTopology:args[0]});
  }
  if (method === "selectAnimationFrameCount") {
    validateSetting("animationFrameCount",args[0],state.animationFrameCount);
    validateMotionConfiguration(state,{animationFrameCount:args[0]});
  }
  return args;
}
