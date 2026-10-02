import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { RuntimeLoader } from "@rive-app/canvas";
import type { Artboard, File, RiveCanvas, StateMachineInstance } from "@rive-app/canvas/rive_advanced.mjs";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

type Cubic = [number, number, number, number, number, number, number, number];
class RecordedPath2D {
  curves: Cubic[] = [];
  x = 0;
  y = 0;
  moveTo(x: number, y: number) { this.x = x; this.y = y; }
  lineTo(x: number, y: number) { this.x = x; this.y = y; }
  closePath() {}
  bezierCurveTo(x1: number, y1: number, x2: number, y2: number, x: number, y: number) {
    this.curves.push([this.x, this.y, x1, y1, x2, y2, x, y]);
    this.x = x; this.y = y;
  }
  addPath(path: RecordedPath2D, matrix: DOMMatrix) {
    for (const curve of path.curves) {
      const transformed = [...curve] as Cubic;
      for (let i = 0; i < 8; i += 2) {
        transformed[i] = matrix.a * curve[i] + matrix.c * curve[i + 1] + matrix.e;
        transformed[i + 1] = matrix.b * curve[i] + matrix.d * curve[i + 1] + matrix.f;
      }
      this.curves.push(transformed);
    }
  }
}

describe("original Jarvito Rive choreography", () => {
  let runtime: RiveCanvas;
  let file: File;
  let artboard: Artboard;
  let machine: StateMachineInstance;
  const require = createRequire(`${process.cwd()}/package.json`);
  const asset = `${process.cwd()}/src/assets/companion/jarvito.riv`;

  beforeAll(async () => {
    vi.stubGlobal("Path2D", RecordedPath2D);
    vi.stubGlobal("DOMMatrix", class { a = 1; b = 0; c = 0; d = 1; e = 0; f = 0; });
    // Import the production WASM parser and animator, with drawing disabled in jsdom.
    // There are no raster meshes in this original all-vector character.
    const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
    const log = console.log.bind(console);
    const meshNotice = vi.spyOn(console, "log").mockImplementation((...args: unknown[]) => {
      if (args[0] !== "No WebGL support. Image mesh will not be drawn.") log(...args);
    });
    try {
      const wasm = readFileSync(require.resolve("@rive-app/canvas/rive.wasm"));
      RuntimeLoader.setWasmBinary(new Uint8Array(wasm).buffer);
      runtime = await RuntimeLoader.awaitInstance();
      file = await runtime.load(new Uint8Array(readFileSync(asset)));
    } finally {
      canvas.mockRestore();
      meshNotice.mockRestore();
    }
  });

  beforeEach(() => {
    artboard = file.artboardByName("Jarvito");
    machine = new runtime.StateMachineInstance(artboard.stateMachineByName("Jarvito"), artboard);
  });
  afterEach(() => { machine?.delete(); artboard?.delete(); });
  afterAll(() => { file?.unref(); vi.unstubAllGlobals(); });

  const advance = (seconds: number) => {
    for (let frame = 0; frame < Math.ceil(seconds * 60); frame++) {
      machine.advanceAndApply(1 / 60);
      artboard.advance(1 / 60);
    }
  };
  const input = (name: string, value: number | boolean) => {
    for (let i = 0; i < machine.inputCount(); i++) {
      const current = machine.input(i);
      if (current.name === name) {
        (typeof value === "boolean" ? current.asBool() : current.asNumber()).value = value;
        return;
      }
    }
    throw new Error(`Jarvito input missing: ${name}`);
  };

  it("ships a reproducible offline file with the complete interaction contract", () => {
    expect(execFileSync("bun", ["scripts/generate-jarvito-rive.mjs", "--check"], { encoding: "utf8" })).toContain("is reproducible");
    expect(file.artboardCount()).toBe(1);
    expect(artboard.bounds).toMatchObject({ minX: 0, minY: 0, maxX: 120, maxY: 120 });
    expect(artboard.animationCount()).toBe(57);
    expect(Array.from({ length: machine.inputCount() }, (_, i) => machine.input(i).name)).toEqual([
      "status", "hovered", "dragging", "reducedMotion", "lookX", "lookY", "expanded", "walking", "gesture", "voiceLevel",
    ]);
    for (const removed of ["Left leg", "Right leg", "Left step", "Right step", "Torso"]) expect(artboard.node(removed)).toBeNull();
  });

  it("listens expressively and moves the speaking mouth with actual audio amplitude", () => {
    input("gesture", 9); advance(2);
    expect(artboard.node("Head emote").rotation).toBeLessThan(-.05);
    input("gesture", 10); input("voiceLevel", 0); advance(.5);
    const closed = artboard.node("Voice amplitude").scaleY;
    input("voiceLevel", 1); advance(.5);
    expect(artboard.node("Voice amplitude").scaleY).toBeGreaterThan(closed);
    expect(artboard.node("Voice mouth reveal").scaleX).toBeCloseTo(1);
    input("gesture", 0); advance(.5);
    expect(artboard.node("Voice mouth reveal").scaleX).toBeCloseTo(0);
  });

  it("renders curved smiles and happy eyes through the actual WASM drawing path", () => {
    const mouthCurves: Cubic[] = [];
    const happyEyeCurves: Cubic[] = [];
    const stroke = (path: unknown) => {
      if (!(path instanceof RecordedPath2D)) throw new Error("Rive did not draw the recorded vector path.");
      if (Math.abs(context.lineWidth - 2.8) < .01) mouthCurves.push(...path.curves);
      if (Math.abs(context.lineWidth - 3.2) < .01) happyEyeCurves.push(...path.curves);
    };
    // Record native Canvas2D drawing commands; rasterization is unnecessary here.
    const canvas = document.createElement("canvas");
    canvas.width = 120; canvas.height = 120;
    const context = {
      canvas,
      save() {}, restore() {}, transform() {}, clip() {}, fill() {}, clearRect() {}, stroke,
      createLinearGradient: () => ({ addColorStop() {} }),
      createRadialGradient: () => ({ addColorStop() {} }),
    } as unknown as CanvasRenderingContext2D;
    const getContext = vi.spyOn(canvas, "getContext").mockReturnValue(context);
    const renderer = runtime.makeRenderer(canvas, false);
    const drawFace = (status: number) => {
      input("status", status);
      advance(4);
      mouthCurves.length = 0; happyEyeCurves.length = 0;
      renderer.clear(); artboard.draw(renderer); renderer.flush(); runtime.resolveAnimationFrame();
      expect(mouthCurves).toHaveLength(1);
      return mouthCurves[0];
    };
    const curvature = ([, y0, , y1, , y2, , y3]: Cubic) => (y1 + y2 - y0 - y3) / 2;
    try {
      const restingSmile = drawFace(0);
      expect(restingSmile[6]).toBeGreaterThan(restingSmile[0]);
      expect(curvature(restingSmile)).toBeGreaterThan(2);
      const happySmile = drawFace(4);
      expect(happySmile[6]).toBeGreaterThan(happySmile[0]);
      expect(curvature(happySmile)).toBeGreaterThan(6);
      expect(happyEyeCurves).toHaveLength(2);
      expect(happyEyeCurves.every((curve) => curvature(curve) < -3)).toBe(true);
      expect(curvature(drawFace(5))).toBeLessThan(-4);
      expect(Math.abs(curvature(drawFace(1)))).toBeLessThan(1);
    } finally { renderer.delete(); getContext.mockRestore(); }
  });

  it("actually animates a thinking pose and follows pointer gaze continuously", () => {
    input("status", 1);
    advance(2);
    expect(artboard.node("Right shoulder").rotation).toBeGreaterThan(1);
    expect(artboard.node("Head").y).toBeLessThan(0);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(.06);
    expect(artboard.node("Left brow").y).toBeLessThan(artboard.node("Right brow").y - 5);
    expect(artboard.node("Left eye").scaleY).toBeLessThan(artboard.node("Right eye").scaleY);
    expect(artboard.node("Eye expression").y).toBeLessThan(-2);
    input("lookX", .5);
    input("lookY", -1);
    advance(.1);
    expect(artboard.node("Gaze X").x).toBeGreaterThan(0);
    expect(artboard.node("Gaze X").x).toBeLessThan(3);
    expect(artboard.node("Gaze Y").y).toBeCloseTo(-2);
    input("lookX", 1);
    advance(.1);
    expect(artboard.node("Gaze X").x).toBeCloseTo(3);
  });

  it("settles celebration and failure instead of repeating alarms forever", () => {
    input("status", 4);
    advance(4);
    expect(artboard.node("Rig").y).toBeCloseTo(70);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(1.35);
    expect(artboard.node("Left eye open").scaleX).toBe(0);
    expect(artboard.node("Left eye happy").scaleX).toBe(1);
    expect(artboard.node("Right eye happy").scaleX).toBe(1);
    const pose = artboard.node("Right shoulder").rotation;
    advance(3);
    expect(artboard.node("Right shoulder").rotation).toBeCloseTo(pose);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(1.35);
    input("status", 5);
    advance(4);
    expect(artboard.node("Head").y).toBeCloseTo(3);
    expect(artboard.node("Head").rotation).toBeCloseTo(.035);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(-.95);
    expect(artboard.node("Left eye open").scaleX).toBe(1);
    expect(artboard.node("Left eye happy").scaleX).toBe(0);
    expect(artboard.node("Left eye").scaleY).toBeCloseTo(.58);
    expect(artboard.node("Left brow").rotation).toBeCloseTo(-.38);
    expect(artboard.node("Right brow").rotation).toBeCloseTo(.38);
    advance(4);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(-.95);
  });

  it("reacts to drag and island state while reduced motion freezes ambient gestures", () => {
    input("dragging", true);
    input("expanded", true);
    advance(2);
    expect(artboard.node("Drag pose").y).toBeCloseTo(-2);
    expect(artboard.node("Attention").scaleY).toBeGreaterThan(1);
    input("walking", true);
    input("dragging", false);
    advance(.55);
    expect(artboard.node("Glide").y).toBeLessThan(0);
    input("reducedMotion", true);
    advance(1 / 60);
    expect(artboard.node("Breathing").y).toBe(0);
    expect(artboard.node("Blink").scaleY).toBe(1);
    expect(artboard.node("Glide").rotation).toBe(0);
    input("status", 1);
    advance(1 / 60);
    expect(artboard.node("Head").y).toBeCloseTo(0);
    expect(artboard.node("Left eye").scaleY).toBeCloseTo(.74);
    advance(5);
    expect(artboard.node("Breathing").y).toBe(0);
    expect(artboard.node("Blink").scaleY).toBe(1);
  });

  it("keeps emotional faces readable while the island, greeting and floating run", () => {
    input("expanded", true);
    input("hovered", true);
    input("walking", true);
    input("status", 4);
    advance(5);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(1.35);
    expect(artboard.node("Left eye happy").scaleX).toBe(1);
    input("status", 5);
    advance(5);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(-.95);
    expect(artboard.node("Right brow").rotation).toBeCloseTo(.38);
    input("status", 1);
    advance(5);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(.06);
    expect(artboard.node("Left brow").y).toBeLessThan(artboard.node("Right brow").y - 5);
  });

  it("uses squash and stretch for celebration, then settles in the same silhouette", () => {
    input("status", 4);
    advance(.25);
    const early = [artboard.node("Rig").scaleX, artboard.node("Rig").scaleY];
    advance(.4);
    expect([artboard.node("Rig").scaleX, artboard.node("Rig").scaleY]).not.toEqual(early);
    advance(4);
    expect(artboard.node("Rig").scaleX).toBeCloseTo(1);
    expect(artboard.node("Rig").scaleY).toBeCloseTo(1);
  });

  it("keeps arms hidden at rest, reveals them for thinking and a greeting, then tucks them away", () => {
    let handDetailStrokes = 0;
    const canvas = document.createElement("canvas");
    const context = {
      canvas,
      save() {}, restore() {}, transform() {}, clip() {}, fill() {}, clearRect() {},
      stroke() { if (Math.abs(context.lineWidth - 1.2) < .01) handDetailStrokes++; },
      createLinearGradient: () => ({ addColorStop() {} }),
      createRadialGradient: () => ({ addColorStop() {} }),
    } as unknown as CanvasRenderingContext2D;
    const getContext = vi.spyOn(canvas, "getContext").mockReturnValue(context);
    const renderer = runtime.makeRenderer(canvas, false);
    const visibleDetails = () => {
      handDetailStrokes = 0;
      renderer.clear(); artboard.draw(renderer); renderer.flush(); runtime.resolveAnimationFrame();
      return handDetailStrokes;
    };
    try {
      advance(2);
      const resting = visibleDetails();
      input("status", 1); advance(2);
      expect(visibleDetails()).toBeGreaterThan(resting);
      input("status", 0); advance(1);
      expect(visibleDetails()).toBe(resting);
      input("hovered", true); advance(.75);
      expect(visibleDetails()).toBeGreaterThan(resting);
      advance(3);
      expect(visibleDetails()).toBe(resting);
    } finally { renderer.delete(); getContext.mockRestore(); }
  });

  it("adds sparse spontaneous facial gestures without a JavaScript animation timer", () => {
    advance(11.6);
    expect(artboard.node("Left eye").scaleY).toBeLessThan(.2);
    expect(artboard.node("Right eye").scaleY).toBeCloseTo(1);
    advance(10.4);
    expect(artboard.node("Left eye").scaleY).toBeLessThan(.5);
    expect(artboard.node("Right eye").scaleY).toBeLessThan(.5);
    advance(5);
    expect(artboard.node("Left eye").scaleY).toBeCloseTo(1);
    expect(artboard.node("Right eye").scaleY).toBeCloseTo(1);
  });

  it("winks, opens a surprised mouth and sleeps without corrupting native expressions", () => {
    input("gesture", 1);
    advance(.5);
    expect(artboard.node("Left eye emote").scaleY).toBeLessThan(.2);
    expect(artboard.node("Right eye emote").scaleY).toBe(1);
    input("gesture", 2);
    advance(.5);
    expect(artboard.node("Left eye emote").scaleY).toBeCloseTo(1.3);
    expect(artboard.node("Right eye emote").scaleY).toBeCloseTo(1.3);
    input("gesture", 3);
    advance(.5);
    expect(artboard.node("Left eye emote").scaleY).toBeCloseTo(.1);
    input("gesture", 0);
    input("status", 4);
    advance(5);
    expect(artboard.node("Left eye emote").scaleY).toBe(1);
    expect(artboard.node("Left eye happy").scaleX).toBe(1);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(1.35);
  });

  it("closes both eyes when poked, rebounds and returns to the ongoing native face", () => {
    input("status", 1);
    advance(2);
    input("gesture", 4);
    advance(.25);
    expect(artboard.node("Left eye emote").scaleY).toBeLessThan(.2);
    expect(artboard.node("Right eye emote").scaleY).toBeLessThan(.2);
    expect(artboard.node("Playful body").scaleY).not.toBeCloseTo(1);
    advance(1.25);
    expect(artboard.node("Left eye emote").scaleY).toBe(1);
    expect(artboard.node("Playful body").scaleY).toBe(1);
    input("gesture", 0);
    advance(.3);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(.06);
  });

  it("wobbles for three seconds with original spiral eyes, then settles without changing the native result", () => {
    input("status", 4);
    advance(5);
    input("gesture", 5);
    advance(.5);
    const early = artboard.node("Playful body").rotation;
    expect(Math.abs(early)).toBeGreaterThan(.1);
    expect(artboard.node("Left spiral eye")).not.toBeNull();
    advance(.5);
    expect(artboard.node("Playful body").rotation).not.toBeCloseTo(early);
    advance(2.2);
    expect(artboard.node("Playful body").rotation).toBe(0);
    expect(artboard.node("Playful body").x).toBe(0);
    input("gesture", 0);
    advance(.3);
    expect(artboard.node("Left eye happy").scaleX).toBe(1);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(1.35);
  });

  it("uses static closed or dizzy eyes in reduced motion without bouncing or rotating", () => {
    input("reducedMotion", true);
    input("gesture", 4);
    machine.advanceAndApply(0); artboard.advance(0);
    expect(artboard.node("Left eye emote").scaleY).toBeCloseTo(.08);
    expect(artboard.node("Right eye emote").scaleY).toBeCloseTo(.08);
    expect(artboard.node("Playful body").scaleY).toBe(1);
    input("gesture", 5);
    machine.advanceAndApply(0); artboard.advance(0);
    expect(artboard.node("Playful body").rotation).toBe(0);
    advance(4);
    expect(artboard.node("Playful body").rotation).toBe(0);
    expect(artboard.node("Playful body").x).toBe(0);
    input("gesture", 0);
    machine.advanceAndApply(0); artboard.advance(0);
    expect(artboard.node("Left eye emote").scaleY).toBe(1);
  });

  it("sleeps with readable closed lids, soft breathing and floating Zs until woken", () => {
    input("gesture", 6);
    advance(1);
    expect(artboard.node("Left eye emote").scaleX).toBe(0);
    expect(artboard.node("Right eye emote").scaleX).toBe(0);
    expect(artboard.node("Sleeping eyes reveal").scaleX).toBe(1);
    expect(artboard.node("Sleeping smile reveal").scaleX).toBe(1);
    expect(artboard.node("Sleep particles").scaleX).toBe(1);
    const earlyPose = [artboard.node("Head emote").rotation, artboard.node("Playful body").scaleY, artboard.node("Sleep particles").y];
    advance(1);
    expect([artboard.node("Head emote").rotation, artboard.node("Playful body").scaleY, artboard.node("Sleep particles").y]).not.toEqual(earlyPose);
    expect(artboard.node("Head emote").rotation).toBeGreaterThan(.12);
    const sleepingPose = [artboard.node("Head emote").rotation, artboard.node("Playful body").scaleY, artboard.node("Sleep particles").y];
    advance(4);
    expect([artboard.node("Head emote").rotation, artboard.node("Playful body").scaleY, artboard.node("Sleep particles").y]).toEqual(sleepingPose);
    input("gesture", 0);
    input("status", 1);
    advance(1);
    expect(artboard.node("Sleep particles").scaleX).toBe(0);
    expect(artboard.node("Sleeping eyes reveal").scaleX).toBe(0);
    expect(artboard.node("Left eye emote").scaleX).toBe(1);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(.06);
  });

  it("unfolds its arms for a wake-up stretch, then tucks them away in two seconds", () => {
    advance(1);
    expect(artboard.node("Left stretch reveal").scaleX).toBe(0);
    input("gesture", 7);
    advance(.65);
    expect(artboard.node("Left stretch reveal").scaleX).toBe(1);
    expect(artboard.node("Right stretch reveal").scaleX).toBe(1);
    expect(artboard.node("Left stretch reveal").rotation).toBeGreaterThan(2);
    expect(artboard.node("Right stretch reveal").rotation).toBeLessThan(-2);
    expect(artboard.node("Playful body").scaleY).toBeGreaterThan(1.01);
    advance(1.7);
    expect(artboard.node("Left stretch reveal").scaleX).toBe(0);
    expect(artboard.node("Right stretch reveal").scaleX).toBe(0);
    expect(artboard.node("Playful body").scaleY).toBe(1);
    expect(artboard.node("Head emote").y).toBe(0);
  });

  it("tilts and glances curiously with asymmetric eyebrows, then restores its native face", () => {
    input("status", 1);
    input("gesture", 8);
    advance(.75);
    expect(artboard.node("Head emote").rotation).toBeLessThan(-.1);
    expect(artboard.node("Gesture gaze").x).toBeLessThan(-1);
    expect(artboard.node("Left brow emote").y).toBeLessThan(-2);
    expect(artboard.node("Right brow emote").y).toBeGreaterThan(.5);
    advance(1.6);
    expect(artboard.node("Head emote").rotation).toBe(0);
    expect(artboard.node("Gesture gaze").x).toBe(0);
    expect(artboard.node("Left brow emote").y).toBe(0);
    expect(artboard.node("Right eye emote").scaleY).toBe(1);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(.06);
  });

  it("keeps the sleeping face static for reduced motion without nodding or moving Zs", () => {
    input("reducedMotion", true);
    input("gesture", 6);
    machine.advanceAndApply(0); artboard.advance(0);
    expect(artboard.node("Sleeping eyes reveal").scaleX).toBe(1);
    expect(artboard.node("Left eye emote").scaleX).toBe(0);
    const restingPose = [artboard.node("Head emote").rotation, artboard.node("Playful body").scaleY, artboard.node("Sleep particles").y];
    advance(10);
    expect([artboard.node("Head emote").rotation, artboard.node("Playful body").scaleY, artboard.node("Sleep particles").y]).toEqual(restingPose);
    expect(artboard.node("Breathing").y).toBe(0);
    input("gesture", 0);
    machine.advanceAndApply(0); artboard.advance(0);
    expect(artboard.node("Sleeping eyes reveal").scaleX).toBe(0);
    expect(artboard.node("Sleep particles").scaleX).toBe(0);
  });

  it("changes the full facial expression immediately with reduced motion", () => {
    input("reducedMotion", true);
    for (const [status, mouthCurve] of [[0, .55], [1, .06], [2, .75], [4, 1.35], [5, -.95]]) {
      input("status", status);
      machine.advanceAndApply(0);
      artboard.advance(0);
      expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(mouthCurve);
      expect(artboard.node("Left eye happy").scaleX).toBe(status === 4 ? 1 : 0);
      expect(artboard.node("Left eye open").scaleX).toBe(status === 4 ? 0 : 1);
    }
    advance(6);
    expect(artboard.node("Mouth expression").scaleY).toBeCloseTo(-.95);
  });
});
