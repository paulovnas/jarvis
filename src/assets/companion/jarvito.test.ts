import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { RuntimeLoader } from "@rive-app/canvas";
import type { Artboard, File, RiveCanvas, StateMachineInstance } from "@rive-app/canvas/rive_advanced.mjs";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

describe("original Jarvito Rive choreography", () => {
  let runtime: RiveCanvas;
  let file: File;
  let artboard: Artboard;
  let machine: StateMachineInstance;
  const require = createRequire(`${process.cwd()}/package.json`);
  const asset = `${process.cwd()}/src/assets/companion/jarvito.riv`;

  beforeAll(async () => {
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
  afterAll(() => { file?.unref(); });

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

  it("actually animates a thinking pose and follows pointer gaze continuously", () => {
    input("status", 1);
    advance(2);
    expect(artboard.node("Right shoulder").rotation).toBeGreaterThan(1);
    expect(artboard.node("Head").y).toBeLessThan(-44);
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
    expect(artboard.node("Left eye").scaleY).toBeCloseTo(.48);
    const pose = artboard.node("Right shoulder").rotation;
    advance(3);
    expect(artboard.node("Right shoulder").rotation).toBeCloseTo(pose);
    input("status", 5);
    advance(4);
    expect(artboard.node("Head").y).toBeCloseTo(-41);
    expect(artboard.node("Head").rotation).toBeCloseTo(.035);
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
    expect(artboard.node("Left eye").scaleY).toBe(1);
    advance(5);
    expect(artboard.node("Breathing").y).toBe(0);
    expect(artboard.node("Blink").scaleY).toBe(1);
  });
});
