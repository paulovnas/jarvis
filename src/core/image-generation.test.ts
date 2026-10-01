import { expect, it } from "vitest";
import { readGeneratedImages } from "./image-generation";

const image = { id: "a1", conversationId: "root-chat", name: "banner.png", mime: "image/png", kind: "image", size: 1200 };
const result = { kind: "generated_image", accountAlias: "configured-images", model: "image-model", images: [image], text: "" };

it("reads existing image history and specialist results with additional ComfyUI metadata", () => {
  expect(readGeneratedImages(JSON.stringify(result))).toEqual(result);
  const delegated = { ...result, agentId: "image-worker", role: "image_generator", sourcePaths: ["output/images/banner.png"], processing: { engine: "comfyui" }, manifest: "output/images/image-task.json" };
  expect(readGeneratedImages(JSON.stringify(delegated))).toEqual({ ...result, processing: { engine: "comfyui" } });
  expect(readGeneratedImages(JSON.stringify({ ...result, processing: { images: [{ width: 4096, height: 2048 }] } }))).toMatchObject({ processing: { images: [{ width: 4096, height: 2048 }] } });
  expect(readGeneratedImages(JSON.stringify({ ...result, processing: { images: [null] } }))).toMatchObject({ images: [image], processing: { images: [null] } });
});

it("rejects unusable generation receipts", () => {
  expect(readGeneratedImages()).toBeNull();
  expect(readGeneratedImages("not json")).toBeNull();
  expect(readGeneratedImages(JSON.stringify({ ...result, images: [] }))).toBeNull();
  expect(readGeneratedImages(JSON.stringify({ ...result, images: [{ ...image, kind: "file" }] }))).toBeNull();
});
