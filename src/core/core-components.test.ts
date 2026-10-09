import { expect, it } from "vitest";
import { coreFixture } from "@/test/core-fixtures";
import { ACTIVE_CORE_IDS, activeCore, coreDownloadEventSchema, coreSnapshotSchema } from "./core-components";

it("accepts the unified OpenMontage Core with only Context7 optional", () => {
  const snapshot = coreFixture();
  const context7 = snapshot.items.find(item => item.id === "context7")!;
  context7.installed = false;
  context7.configured = false;
  context7.installedVersion = null;
  expect(coreSnapshotSchema.safeParse(snapshot).success).toBe(true);
  expect(snapshot.items).toHaveLength(9);
  expect(coreDownloadEventSchema.parse({ id: "openmontage", download: { receivedBytes: 4096, totalBytes: null } }).id).toBe("openmontage");
  expect(coreDownloadEventSchema.parse({ id: "audiovisual", download: { receivedBytes: 4096, totalBytes: null } }).id).toBe("audiovisual");
  expect(coreDownloadEventSchema.parse({ id: "comfyui", download: { receivedBytes: 4096, totalBytes: null } }).id).toBe("comfyui");
  expect(coreDownloadEventSchema.parse({ id: "graft", download: { receivedBytes: 4096, totalBytes: null } }).id).toBe("graft");
});

it.each(["impeccable", "openmontage", "comfyui", "graft"] as const)("requires %s for Core readiness", id => {
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
  expect(coreSnapshotSchema.safeParse({ ...snapshot, items: snapshot.items.filter(item => item.id !== "openmontage") }).success).toBe(false);
  expect(coreSnapshotSchema.safeParse({ ...snapshot, items: [...snapshot.items.slice(0, -1), snapshot.items[0]] }).success).toBe(false);
});

it("preserves complete historical Core snapshots without accepting a partial migration", () => {
  const snapshot = coreFixture();
  const video = snapshot.items.find(item => item.id === "openmontage")!;
  const legacy = { ...snapshot, items: [...snapshot.items.filter(item => item.id !== "openmontage"), { ...video, id: "hyperframes", name: "Hyperframes" }, { ...video, id: "audiovisual", name: "Audiovisual" }] };
  expect(coreSnapshotSchema.safeParse(legacy).success).toBe(true);
  expect(coreSnapshotSchema.safeParse({ ...snapshot, items: [...snapshot.items, legacy.items[legacy.items.length - 1]] }).success).toBe(false);
});

it("replaces OpenDesign in active Core resources while preserving historical snapshots", () => {
  const snapshot = coreFixture();
  const design = snapshot.items.find(item => item.id === "impeccable")!;
  const legacy = { ...snapshot, items: snapshot.items.map(item => item.id === "impeccable" ? { ...item, id: "open-design", name: "Open Design" } : item) };
  expect(ACTIVE_CORE_IDS).toContain("impeccable");
  expect(activeCore("open-design")).toBe(false);
  expect(coreSnapshotSchema.safeParse(legacy).success).toBe(true);
  expect(coreSnapshotSchema.safeParse({ ...snapshot, items: [...snapshot.items, { ...design, id: "open-design" }] }).success).toBe(false);
});
