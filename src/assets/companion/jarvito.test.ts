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
    expect(artboard.bounds).toMatchObject({ minX: 0, minY: 0, maxX: 120, maxY: 140 });
    expect(artboard.animationCount()).toBe(32);
    expect(Array.from({ length: machine.inputCount() }, (_, i) => machine.input(i).name)).toEqual([
      "status", "hovered", "dragging", "reducedMotion", "lookX", "lookY", "expanded", "walking",
    ]);
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
    canvas.width = 120; canvas.height = 140;
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
    expect(artboard.node("Head").y).toBeLessThan(-44);
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
    expect(artboard.node("Rig").y).toBeCloseTo(112);
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
    expect(artboard.node("Head").y).toBeCloseTo(-41);
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
    expect(artboard.node("Left step").rotation).not.toBeCloseTo(artboard.node("Right step").rotation);
    input("reducedMotion", true);
    advance(1 / 60);
    expect(artboard.node("Breathing").y).toBe(0);
    expect(artboard.node("Blink").scaleY).toBe(1);
    expect(artboard.node("Left step").rotation).toBe(0);
    input("status", 1);
    advance(1 / 60);
    expect(artboard.node("Head").y).toBeCloseTo(-44);
    expect(artboard.node("Left eye").scaleY).toBeCloseTo(.74);
    advance(5);
    expect(artboard.node("Breathing").y).toBe(0);
    expect(artboard.node("Blink").scaleY).toBe(1);
  });

  it("keeps emotional faces readable while the island, greeting and walking run", () => {
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
