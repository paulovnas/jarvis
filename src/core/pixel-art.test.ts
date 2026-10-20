import { expect, it } from "vitest";
import { pixelArtResultSchema, readPixelArtResult } from "./pixel-art";
import { builtinAgentSchema, flowOptions, workflowStepSchema } from "./workflow-catalog";

it("accepts the dedicated pixel art agent both directly and as a workflow node", () => {
  const agent = builtinAgentSchema.safeParse({
    id: "builtin:pixel_art", name: "Artista de pixel art", description: "Pixel art e animações",
    instructions: "Use pixel_art_docs e as ferramentas do motor incorporado.",
    role: "pixel_art", usage: "mixed", capability: "commands",
    appearance: { icon: "palette", color: "purple" }, immutable: true,
  });
  expect(agent.success).toBe(true);
  expect(flowOptions("agent:builtin:pixel_art")).toEqual({ workflow: "custom", customAgentId: "builtin:pixel_art" });
  expect(workflowStepSchema.safeParse({
    id: "a".repeat(32), agentId: "builtin:pixel_art", instructions: "",
    position: { x: 0, y: 0 }, next: null, onRework: null,
  }).success).toBe(true);
});

it("preserves animated image metadata and rejects malformed or impossible progress", () => {
  const data = { kind: "pixel_art", jobId: "job", action: "export", status: "completed", phase: "Pronto", images: [{
    id: "gif", conversationId: "chat", name: "walk.gif", mime: "image/gif", kind: "image", size: 100,
  }] };
  expect(readPixelArtResult(JSON.stringify(data))?.images[0].mime).toBe("image/gif");
  expect(pixelArtResultSchema.safeParse({ ...data, progress: { completed: 3, total: 2, unit: "direções" } }).success).toBe(false);
  expect(readPixelArtResult("{")).toBeNull();
  expect(readPixelArtResult(JSON.stringify({ ...data, status: "made-up" }))).toBeNull();
});
