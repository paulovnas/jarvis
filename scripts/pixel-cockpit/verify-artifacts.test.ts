import { describe, expect, it } from "vitest";
import JSZip from "jszip";
import { inspectArtifactBytes } from "./verify-artifacts";

const onePixelGif = Buffer.from("R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7", "base64");

describe("Pixel Cockpit artifact evidence", () => {
  it("counts actual GIF image blocks and retains individual delays", async () => {
    const controlAndFrame = onePixelGif.subarray(19, onePixelGif.length - 1);
    const second = Buffer.from(controlAndFrame);
    second.writeUInt16LE(12, 4);
    const animated = Buffer.concat([onePixelGif.subarray(0, 19), controlAndFrame, second, Buffer.from([0x3b])]);
    const result = await inspectArtifactBytes("actual.gif", animated);
    expect(result).toMatchObject({ format: "gif", width: 1, height: 1, frames: 2, frameDurationsMs: [0, 120], alphaChannel: true });
    expect(result.sha256).toMatch(/^[a-f0-9]{64}$/);
  });

  it("rejects a GIF truncated before its frame data or trailer", async () => {
    await expect(inspectArtifactBytes("broken.gif", onePixelGif.subarray(0, -4))).rejects.toThrow(/Truncated|trailer/);
  });

  it("checks pack references against real ZIP entries and its schema", async () => {
    const zip = new JSZip();
    zip.file("manifest.json", JSON.stringify({ schema: "image-cockpit.animation.v2", files: { sheet: "sheet.png", engine: { godot: "engine/godot.json" } } }));
    zip.file("sheet.png", Buffer.from("fixture"));
    zip.file("engine/godot.json", "{}");
    const result = await inspectArtifactBytes("pack.zip", await zip.generateAsync({ type: "nodebuffer" }));
    expect(result).toMatchObject({ format: "zip", schema: "image-cockpit.animation.v2", manifestReferences: ["sheet.png", "engine/godot.json"] });
    zip.remove("engine/godot.json");
    await expect(inspectArtifactBytes("incomplete.zip", await zip.generateAsync({ type: "nodebuffer" }))).rejects.toThrow("missing file: engine/godot.json");
  });

  it("rejects unsafe paths even when the ZIP reader normalizes them", async () => {
    const zip = new JSZip();
    zip.file("../escaped.json", "{}");
    await expect(inspectArtifactBytes("unsafe.zip", await zip.generateAsync({ type: "nodebuffer" }))).rejects.toThrow("Unsafe archive path");
  });

  it("checks v1 directional previews without treating direction identifiers as filenames", async () => {
    const zip = new JSZip();
    zip.file("manifest.json", JSON.stringify({
      schema: "image-cockpit.animation.v1",
      files: { sheet: "sheet.png", directionPreviews: [{ direction: "front", gif: "front.gif" }] },
    }));
    zip.file("sheet.png", Buffer.from("fixture"));
    zip.file("front.gif", onePixelGif);
    const result = await inspectArtifactBytes("v1.zip", await zip.generateAsync({ type: "nodebuffer" }));
    expect(result.manifestReferences).toEqual(["sheet.png", "front.gif"]);
    zip.remove("front.gif");
    await expect(inspectArtifactBytes("incomplete-v1.zip", await zip.generateAsync({ type: "nodebuffer" }))).rejects.toThrow("missing file: front.gif");
  });

  it("reports JSON metadata without claiming visual or real-generation acceptance", async () => {
    const result = await inspectArtifactBytes("metadata.json", Buffer.from('{"schema":"image-cockpit.effect-animation.v1"}'));
    expect(result).toMatchObject({ format: "json", schema: "image-cockpit.effect-animation.v1" });
    expect(result).not.toHaveProperty("passed");
    expect(result).not.toHaveProperty("generated");
  });
});
