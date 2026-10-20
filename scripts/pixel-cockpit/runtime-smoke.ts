type Data = Record<string, unknown>;

export interface PixelRuntime {
  start(request: { id: string; action: string; args?: Data }): unknown | Promise<unknown>;
  snapshot(id?: string): unknown | Promise<unknown>;
}

export interface PixelVerificationOptions {
  /** Explicitly opt into four real Codex generation jobs; no procedural fallback. */
  runRealGeneration?: boolean;
  /** Reuse generated assets when checking exports/editors without additional generation. */
  sourceId?: string;
  animationId?: string;
  effectId?: string;
  timeoutMs?: number;
  pollMs?: number;
  onProgress?: (receipt: Data) => void;
}

export interface PixelVerificationReport {
  startedAt: string;
  finishedAt: string;
  status: "passed" | "blocked";
  operations: Data[];
  assets: { sourceId?: string; editedId?: string; animationId?: string; effectId?: string };
  unverified: string[];
  failures: { scenario: string; error: string }[];
  error?: string;
}

const asRecord = (value: unknown): Data => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Data : {};
const asArray = (value: unknown): unknown[] => Array.isArray(value) ? value : [];
const sleep = (ms: number) => new Promise<void>((done) => setTimeout(done, ms));

function requireString(value: unknown, label: string) {
  if (typeof value !== "string" || !value) throw new Error(`Missing ${label}.`);
  return value;
}

/**
 * Runs on the real suite bridge, using its actual controllers, image bytes and
 * renderer. This is runtime operation coverage, not a visual-quality verdict.
 */
export async function runPixelCockpitVerification(runtime: PixelRuntime, options: PixelVerificationOptions = {}): Promise<PixelVerificationReport> {
  const report: PixelVerificationReport = {
    startedAt: new Date().toISOString(),
    finishedAt: "",
    status: "blocked",
    operations: [],
    assets: { sourceId: options.sourceId, animationId: options.animationId, effectId: options.effectId },
    unverified: [
      "Human visual acceptance and moving GIF playback inside the Jarvis transcript.",
      "Manual suite interaction interleaved with agent commands in the same native session.",
      "Experimental Motion Pilot, batch matrix, paid direction repair and runner cancellation/reload require separate live scenarios.",
    ],
    failures: [],
  };
  const prefix = `parity-${Date.now().toString(36)}`;
  let sequence = 0;
  const run = async (action: string, args: Data = {}): Promise<Data> => {
    const id = `${prefix}-${++sequence}`;
    const started = Date.now();
    await runtime.start({ id, action, args });
    for (;;) {
      const receipt = asRecord(await runtime.snapshot(id));
      if (typeof receipt.status !== "string") throw new Error(`Missing runtime receipt for ${id}.`);
      options.onProgress?.(receipt);
      if (receipt.status !== "running") {
        report.operations.push({ ...receipt, request: { action, args } });
        if (receipt.status !== "completed") throw new Error(`${action}: ${String(receipt.error ?? receipt.phase ?? receipt.status)}`);
        return receipt;
      }
      if (Date.now() - started > (options.timeoutMs ?? 45 * 60 * 1000)) {
        report.operations.push({ ...receipt, request: { action, args } });
        throw new Error(`${action} is still running after the verification deadline. Inspect ${id}; do not resubmit automatically.`);
      }
      await sleep(options.pollMs ?? 1000);
    }
  };
  const invoke = (method: string, args: unknown[] = [], extra: Data = {}) => run("invoke", { method, args, ...extra });
  const state = async (area = "app") => asRecord((await run("state", { controller: area })).result);
  const reference = (name: string, area = "app") => ({ state: name, controller: area });
  const saved = (name: string) => ({ saved: name });
  const commitTimeline = async (method: string, args: unknown[]) => {
    await invoke(method, [reference("draft", "timeline"), ...args], { module: "animationPackV2", saveAs: "timeline-edit" });
    await invoke("commit", [saved("timeline-edit")], { controller: "timeline" });
  };
  const selectedId = async () => requireString(asRecord((await state()).selected).id, "selected history ID");
  const attempt = async (scenario: string, operation: () => Promise<void>) => {
    try { await operation(); }
    catch (error) { report.failures.push({ scenario, error: error instanceof Error ? error.message : String(error) }); }
  };

  try {
    const ready = asRecord(await runtime.snapshot());
    if (!ready.ready) throw new Error("The real Pixel Cockpit suite is not ready.");
    await run("catalog");
    await invoke("runCockpitHealthCheck", [{ quiet: true }]);
    await invoke("MOTION_RECIPES", [], { module: "motionRecipes", saveAs: "all-motion-recipes" });
    await invoke("BODY_TOPOLOGY_PROFILES", [], { module: "motionRecipes" });
    await invoke("ANIMATION_GENERATION_PROFILES", [], { module: "animationTournament" });

    if (options.runRealGeneration) {
      await attempt("Real pixel-art generation", async () => {
      await run("generate", {
        workflow: "image-generate",
        parameters: {
          prompt: "A single original blue clockwork courier robot, full body side view, readable 32-bit pixel art, tiny amber backpack, two clear feet, simple separated silhouette, transparent background. No text, no border, no duplicates.",
          negativePrompt: "blur, watermark, cropped feet, multiple characters, text",
          count: 1,
          size: "1024x1024",
          quality: "auto",
        },
      });
      report.assets.sourceId = await selectedId();
      await invoke("adoptSelected");
      await run("inspect");
      });
      if (report.assets.sourceId) {
      await attempt("Real image editing", async () => {
      await run("select", { id: report.assets.sourceId });
      await run("annotate", {
        rect: { x: 0.2, y: 0.2, width: 0.6, height: 0.5 },
        comment: "Make the backpack amber-orange and preserve the character identity.",
      });
      await run("generate", {
        workflow: "image-edit",
        parameters: { prompt: "Change only the backpack to warm amber-orange. Preserve all other pixels, pose, proportions and transparent background.", count: 1 },
      });
      report.assets.editedId = await selectedId();
      if (report.assets.editedId === report.assets.sourceId) throw new Error("Image editing did not create a separate derivative.");
      await run("inspect");
      });
      await attempt("Real sprite animation", async () => {
      await run("select", { id: report.assets.sourceId });
      await run("preset", { kind: "animation", id: "walk-cycle" });
      await run("generate", {
        workflow: "sprite-generate",
        parameters: {
          animationSourceId: report.assets.sourceId,
          activeActionName: "walk",
          animationDirectionPreset: "one",
          animationSingleDirection: "side",
          animationFrameCount: 4,
          animationBodyTopology: "biped",
          animationGenerationProfile: "fast",
          motionPilotEnabled: false,
        },
      });
      report.assets.animationId = await selectedId();
      const animation = asRecord((await state()).selected);
      if (report.assets.animationId === report.assets.sourceId
        || asRecord(animation.motionRecipe).frameCount !== 4
        || !asArray(animation.animationDirections).includes("side")) {
        throw new Error("The generated result is not the requested four-frame side animation with retained recipe metadata.");
      }
      });
      }
      await attempt("Real effect animation", async () => {
      await run("generate", {
        workflow: "effect-animation",
        parameters: {
          effectCategoryId: "hit-spark", effectTypeId: "burst", effectStyleId: "pixel-clean",
          effectPaletteId: "ember-orange", effectFrameCount: 4, effectCanvasSize: 96,
          effectLayoutId: "horizontal-strip", effectLoopMode: "one-shot", effectAnchorMode: "center",
          prompt: "A compact orange pixel-art impact spark with a clear flash then fading particles, clean transparent background, no character.",
          count: 1,
        },
      });
      report.assets.effectId = await selectedId();
      const effect = asRecord(asRecord((await state()).selected).effectAnimation);
      if (effect.frameCount !== 4) throw new Error("The effect result does not retain the requested four-frame metadata.");
      });
    } else {
      report.unverified.push("Real generation was not requested by this verification invocation; input artifacts were reused.");
    }

    const animationId = report.assets.animationId;
    const effectId = report.assets.effectId;
    if (!options.runRealGeneration) {
      requireString(animationId, "real animation ID");
      requireString(effectId, "real effect ID");
    }
    if (animationId) {
    await attempt("Animation exports, timeline and review", async () => {
    await run("select", { id: animationId });
    await run("inspect");
    for (const format of ["image", "direction-gif", "direction-apng", "direction-webp", "animation-pack"]) {
      await run("export", { format });
    }
    const animationState = await state();
    const action = asRecord(animationState.selectedAnimationAction);
    const frameIds = asArray(action.frameIds).map((id) => ({ ref: "frames", id: requireString(id, "frame ID") }));
    if (frameIds.length < 2) throw new Error("Animation has fewer than two addressable frames.");
    await invoke("exportFramesZip", [frameIds, reference("selectedAnimationAction")], { module: "exporters" });
    await invoke("exportMetadata", ["parity-animation", [reference("selectedAnimationAction")], frameIds], { module: "exporters" });
    await run("timeline_open");
    await run("configure", { controller: "timeline", settings: { playing: false } });
    await commitTimeline("setAnimationFrameDuration", [0, 125]);
    await commitTimeline("holdAnimationFrame", [0, 100]);
    await commitTimeline("duplicateAnimationFrame", [0]);
    await commitTimeline("reverseAnimationTimeline", []);
    await commitTimeline("setAnimationLoopMode", ["ping-pong"]);
    for (const kind of ["pivot", "anchor", "socket"]) await commitTimeline("addPackPoint", [kind, 0, `parity-${kind}`]);
    for (const kind of ["hitbox", "hurtbox"]) await commitTimeline("addPackRect", [kind, 0, `parity-${kind}`]);
    await invoke("undo", [], { controller: "timeline" });
    await invoke("redo", [], { controller: "timeline" });
    const timeline = asRecord((await state("timeline")).draft);
    const point = asRecord(asArray(timeline.pivots)[0]);
    const box = asRecord(asArray(timeline.hitboxes)[0]);
    if (point.id) await invoke("updatePoint", [point, "x", 0.45], { controller: "timeline" });
    if (box.id) await invoke("updateRect", [box, "width", 0.4], { controller: "timeline" });
    const event = asRecord(asArray(timeline.events)[0]);
    if (event.id) await commitTimeline("moveAnimationEvent", [event.id, 1]);
    await invoke("saveAnimationTimelinePack", [reference("draft", "timeline")]);
    const roundTrip = await invoke("verifyAnimationPackV2RoundTrip", [reference("draft", "timeline")]);
    requireString(roundTrip.result, "successful timeline round-trip confirmation");
    await invoke("exportSelectedAnimationPackV2", [reference("draft", "timeline")]);
    const engines = asArray((await invoke("buildAnimationPackV2EngineExports", [reference("draft", "timeline")], { module: "animationPackV2" })).result);
    if (engines.length !== 5) throw new Error("Expected five original animation engine metadata exports.");
    for (const entry of engines) {
      const output = asRecord(entry);
      await invoke("exportAnimationEngineMetadata", [output.filename, output.data]);
    }
    await invoke("close", [], { controller: "timeline" });

    const monitors = asArray(animationState.animationTournamentMonitors);
    const manifest = asRecord(asRecord(monitors[0]).manifest);
    if (typeof manifest.tournamentId === "string") {
      await run("review_open", { tournamentId: manifest.tournamentId });
      await run("configure", { controller: "review", settings: { playing: false, showOnion: true, showDiff: true, showGround: true, showPivot: true, sourceOpacity: 0.3 } });
      await invoke("exportCalibration", [], { controller: "review" });
      await invoke("close", [], { controller: "review" });
    } else {
      report.unverified.push("No retained real tournament manifest was available for the review/calibration scenario.");
    }
    });
    } else { report.unverified.push("Animation editor/export scenarios require a successfully generated animation."); }
    if (effectId) {
    await attempt("Effect exports", async () => {
    await run("select", { id: effectId });
    for (const format of ["effect-gif", "effect-apng", "effect-sheet", "effect-frames", "effect-metadata", "effect-pack"]) {
      await run("export", { format });
    }
    });
    } else { report.unverified.push("Effect export scenarios require a successfully generated effect."); }
    if (animationId && effectId) {
    await attempt("Character and effect composition", async () => {
    await run("vfx_open");
    await invoke("updateCharacter", [animationId], { controller: "vfx" });
    await invoke("updateEffect", [effectId], { controller: "vfx" });
    await invoke("updateTiming", ["impact", "peak"], { controller: "vfx" });
    await invoke("patch", [{ layer: "front", blendMode: "additive", transform: { offsetX: 3, offsetY: -2, scale: 0.7, rotation: 5, opacity: 0.85 } }], { controller: "vfx" });
    await invoke("undo", [], { controller: "vfx" });
    await invoke("redo", [], { controller: "vfx" });
    await invoke("saveSetup", [], { controller: "vfx" });
    for (const format of ["gif", "apng", "sheet", "pack"]) await invoke("exportArtifacts", [format], { controller: "vfx" });
    await invoke("exportManifest", [], { controller: "vfx" });
    await invoke("exportLayers", [], { controller: "vfx" });
    await invoke("close", [], { controller: "vfx" });
    });
    } else { report.unverified.push("VFX composition requires both a generated animation and effect."); }
    await invoke("recoverUnimportedOutboxResults");
    await invoke("dedupeLocalInboxHistoryNow");
    await run("state");
    report.status = report.failures.length ? "blocked" : "passed";
    if (report.failures.length) report.error = report.failures.map((failure) => `${failure.scenario}: ${failure.error}`).join("\n");
  } catch (error) {
    report.error = error instanceof Error ? error.message : String(error);
  }
  report.finishedAt = new Date().toISOString();
  return report;
}
