import { describe, expect, it } from "vitest";
import { isPublicModuleMember, resolveInvocationArguments, selectCatalogMethod, settingApplicationGroups, validateCallable, validateMotionConfiguration, validateSetting, validateSuiteSetting } from "./contract";

describe("Pixel Art public commands", () => {
  it("rejects recipe-incompatible topology and uses catalog objects before changing state", () => {
    const preset = {id:"idle-breathing",title:{en:"Idle"},frameCount:8};
    const state = {animationBodyTopology:"biped",animationFrameCount:8,animationPresets:[preset],
      selectedMotionRecipe:{id:"idle-breathing",supportedTopologies:["biped"],allowedFrameCounts:[4,8]}};
    expect(() => resolveInvocationArguments("selectAnimationBodyTopology",["quadruped"],state)).toThrow(/não aceita/);
    expect(() => validateMotionConfiguration(state,{animationBodyTopology:"quadruped"})).toThrow(/biped/);
    expect(resolveInvocationArguments("useAnimationPresetExample",[{id:"idle-breathing",frameCount:999}],state)[0]).toBe(preset);
    expect(() => resolveInvocationArguments("useAnimationPresetExample",[{id:"invented"}],state)).toThrow(/existente/);
    expect(state.animationBodyTopology).toBe("biped");
  });
  it("applies effect preset selection before requested frame and canvas overrides", () => {
    const groups = settingApplicationGroups([
      ["effectFrameCount", 4], ["effectCanvasSize", 96],
      ["effectCategoryId", "hit-spark"], ["effectTypeId", "burst"], ["effectLoopMode", "loop"],
    ] as Array<[string, string | number]>);
    expect(groups).toEqual([
      [["effectCategoryId", "hit-spark"], ["effectTypeId", "burst"]],
      [["effectFrameCount", 4], ["effectCanvasSize", 96], ["effectLoopMode", "loop"]],
    ]);
  });
  it("returns the selected module method and only its referenced argument fields", () => {
    const type = {source:"src/types.ts",definition:"interface Anchor {x:number;y:number}"};
    const result = selectCatalogMethod({
      methods:{edit:{parameters:"wrong: string"}},
      modules:{timeline:{edit:{parameters:"anchor: Anchor"}}},
      types:{Anchor:[type],Unrelated:[{definition:"interface Unrelated {}"}]},
    }, {module:"timeline",method:"edit"});
    expect(result.schema).toEqual({parameters:"anchor: Anchor"});
    expect(result.relatedTypes).toEqual({Anchor:[type]});
  });
  it("keeps JSON and scoped-file adapters while hiding raw browser objects", () => {
    expect(isPublicModuleMember("image", "canvasToBlob", "canvas: HTMLCanvasElement")).toBe(false);
    expect(isPublicModuleMember("image", "loadImage", "source: string")).toBe(false);
    expect(isPublicModuleMember("storage", "saveJson", "key:string,value:T")).toBe(false);
    expect(isPublicModuleMember("animationPack", "importAnimationPackBlob", "blob: Blob")).toBe(true);
    expect(isPublicModuleMember("animationPackV2", "moveAnimationEvent", "pack: AnimationPackV2, eventId: string, frameIndex: number")).toBe(true);
  });
  it("rejects invalid frame counts, types, ranges and fabricated internal outcomes", () => {
    expect(() => validateSetting("animationFrameCount", 7, 8)).toThrow();
    expect(() => validateSetting("count", 9, 1)).toThrow();
    expect(() => validateSetting("count", "2", 1)).toThrow();
    expect(() => validateSetting("history", [], [])).toThrow("estado interno");
    expect(() => validateSetting("codexFailureNotices", [], [])).toThrow();
    expect(() => validateSetting("grid", { columns: 0, rows: 3, gutter: 0 }, {})).toThrow();
    expect(() => validateSetting("animationFrameCount", 4, 8)).not.toThrow();
    expect(() => validateSetting("tool", "unknown", "select")).toThrow();
    expect(() => validateSetting("animationMotionVariant", null, {speed:"normal"})).toThrow();
    expect(() => validateSetting("batchMatrixSourceIds", ["duplicate", "duplicate"], [])).toThrow();
    expect(() => validateSetting("animationPackExportDraft", {title:"Missing fields"}, {})).toThrow();
  });

  it("exposes user operations while excluding callbacks and DOM-only helpers", () => {
    expect(() => validateCallable("splitSelectedToTimeline", "")).not.toThrow();
    expect(() => validateCallable("pollCodexJobForReturnedImage", "job: Job, isCancelled: () => boolean")).toThrow("helper interno");
    expect(() => validateCallable("openVfxCompositeStage", "trigger?: HTMLElement")).toThrow("interface");
  });

  it("allows bounded playback and overlays without fabricating review or timeline history", () => {
    const state = { frameIndex: 0, playing: false, speed: 1, draft: { frameOrder: [0, 1, 2, 3], directions: ["front"] } };
    expect(() => validateSuiteSetting("timeline", "frameIndex", 3, state)).not.toThrow();
    expect(() => validateSuiteSetting("timeline", "frameIndex", 4, state)).toThrow();
    expect(() => validateSuiteSetting("timeline", "speed", 0.5, state)).not.toThrow();
    expect(() => validateSuiteSetting("timeline", "draft", {}, state)).toThrow("estado interno");
    expect(() => validateSuiteSetting("review", "review", {}, {})).toThrow("estado interno");
  });
});
