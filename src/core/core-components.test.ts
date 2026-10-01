import { expect, it } from "vitest";
import { coreFixture } from "@/test/core-fixtures";
import { coreDownloadEventSchema, coreSnapshotSchema } from "./core-components";

it("accepts the ten-component Core with Graft and only Context7 optional", () => {
  const snapshot = coreFixture();
  const context7 = snapshot.items.find(item => item.id === "context7")!;
  context7.installed = false;
  context7.configured = false;
  context7.installedVersion = null;
  expect(coreSnapshotSchema.safeParse(snapshot).success).toBe(true);
  expect(snapshot.items).toHaveLength(10);
  expect(coreDownloadEventSchema.parse({ id: "audiovisual", download: { receivedBytes: 4096, totalBytes: null } }).id).toBe("audiovisual");
  expect(coreDownloadEventSchema.parse({ id: "comfyui", download: { receivedBytes: 4096, totalBytes: null } }).id).toBe("comfyui");
  expect(coreDownloadEventSchema.parse({ id: "graft", download: { receivedBytes: 4096, totalBytes: null } }).id).toBe("graft");
});

it.each(["hyperframes", "audiovisual", "comfyui", "graft"] as const)("requires %s for Core readiness", id => {
  const snapshot = coreFixture();
  const item = snapshot.items.find(item => item.id === id)!;
  item.installed = false;
  item.configured = false;
  item.installedVersion = null;
  expect(coreSnapshotSchema.safeParse(snapshot).success).toBe(false);
  snapshot.ready = false;
  expect(coreSnapshotSchema.safeParse(snapshot).success).toBe(true);
});

it("requires a healthy Graft runtime even when its package is installed", () => {
  const snapshot = coreFixture();
  snapshot.items.find(item => item.id === "graft")!.healthError = "O parser estrutural não está disponível.";
  expect(coreSnapshotSchema.safeParse(snapshot).success).toBe(false);
  snapshot.ready = false;
  expect(coreSnapshotSchema.safeParse(snapshot).success).toBe(true);
});

it("rejects incomplete or duplicate component snapshots instead of releasing onboarding", () => {
  const snapshot = coreFixture();
  expect(coreSnapshotSchema.safeParse({ ...snapshot, items: snapshot.items.filter(item => item.id !== "audiovisual") }).success).toBe(false);
  expect(coreSnapshotSchema.safeParse({ ...snapshot, items: [...snapshot.items.slice(0, -1), snapshot.items[0]] }).success).toBe(false);
});
