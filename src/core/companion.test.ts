import { expect, it } from "vitest";
import { companionGeometrySchema, companionSnapshotSchema } from "./companion";

it("accepts snapshots without a task plan and validates actual task states", () => {
  const item = {
    conversationId: "chat", agentId: null, projectId: "project", projectName: "Portal", title: "Revisar a tela", role: "builder", status: "running",
    activity: "Verificando", durationMs: 1000, activeSince: 10, updatedAt: 20, requiresConversation: false, attentionId: "chat/running", acknowledged: false,
  };
  expect(companionSnapshotSchema.parse({ items: [item], truncated: false }).items[0].tasks).toEqual([]);
  const tasks = [{ id: "ui", title: "Revisar os componentes", status: "blocked" }];
  expect(companionSnapshotSchema.parse({ items: [{ ...item, tasks }], truncated: false }).items[0].tasks).toEqual(tasks);
  expect(companionSnapshotSchema.safeParse({ items: [{ ...item, tasks: [{ ...tasks[0], status: "failed" }] }], truncated: false }).success).toBe(false);
});

it("keeps the compact island origin in the larger native canvas for smooth closing", () => {
  const geometry = companionGeometrySchema.parse({
    expanded: true, bubble: false, robotSide: "left", robotVertical: "top",
    width: 640, height: 160, compactX: 176, compactY: 0, compactWidth: 288, compactHeight: 32,
    surfaceX: 0, surfaceY: 0, surfaceWidth: 640, surfaceHeight: 160,
  });
  expect(geometry).toMatchObject({ compactX: 176, compactY: 0, compactWidth: 288, compactHeight: 32, surfaceWidth: 640, surfaceHeight: 160 });
  expect(companionGeometrySchema.parse({ expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 288, height: 32 }))
    .toMatchObject({ compactX: 0, compactY: 0, compactWidth: 288, compactHeight: 32, surfaceWidth: 288, surfaceHeight: 32 });
});

it("preserves the camera exclusion area and platform drag policy", () => {
  const geometry = companionGeometrySchema.parse({
    expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 314, height: 38,
    notchWidth: 210, notchHeight: 38, headerHeight: 38, dragAxis: "none",
  });
  expect(geometry).toMatchObject({ notchWidth: 210, notchHeight: 38, headerHeight: 38, dragAxis: "none" });
  expect(companionGeometrySchema.safeParse({ ...geometry, dragAxis: "vertical" }).success).toBe(false);
});
