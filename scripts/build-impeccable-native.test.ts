import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { publishImpeccableNative, type NativeStage } from "./build-impeccable-native";
import { IMPECCABLE_ENGINE_VERSION, IMPECCABLE_SOURCE_COMMIT, nativeEnginePatches } from "./prepare-impeccable";

const directories: string[] = [];
afterEach(async () => { await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true, force: true }))); });

async function fixture(): Promise<{ stage: NativeStage; destination: string }> {
  const directory = await mkdtemp(path.join(os.tmpdir(), "jarvis-native-receipt-test-"));
  directories.push(directory);
  const stage: NativeStage = { directory, target: "darwin-arm64", triple: "aarch64-apple-darwin", binary: path.join(directory, "target/aarch64-apple-darwin/release/impeccable") };
  await mkdir(path.dirname(stage.binary), { recursive: true });
  await writeFile(stage.binary, "isolated test engine bytes");
  await writeFile(path.join(directory, "jarvis-stage.json"), JSON.stringify({ sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: stage.target, patches: await nativeEnginePatches() }));
  return { stage, destination: path.join(directory, "published") };
}

describe("native engine provenance publication", () => {
  it("rejects a staging receipt lacking canonical design support even with the current queue patch", async () => {
    const { stage, destination } = await fixture();
    await writeFile(path.join(stage.directory, "jarvis-stage.json"), JSON.stringify({ sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: stage.target, patches: (await nativeEnginePatches()).slice(0, 1) }));
    await expect(publishImpeccableNative(stage, destination)).rejects.toThrow("não corresponde");
    await expect(stat(destination)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("publishes a receipt for the exact engine bytes and tracked patch set", async () => {
    const { stage, destination } = await fixture();
    const result = await publishImpeccableNative(stage, destination);
    expect(await readFile(result.enginePath, "utf8")).toBe("isolated test engine bytes");
    const receipt: unknown = JSON.parse(await readFile(path.join(destination, "bin/jarvis-engine.json"), "utf8"));
    expect(receipt).toEqual({ schemaVersion: 1, engineVersion: IMPECCABLE_ENGINE_VERSION, cliVersion: "4.0.0", sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: "darwin-arm64", patches: await nativeEnginePatches(), engineSha256: createHash("sha256").update(await readFile(stage.binary)).digest("hex") });
  });

  it("rejects an engine receipt missing hot design updates while retaining the old bindings", async () => {
    const { stage, destination } = await fixture();
    const patches = (await nativeEnginePatches()).filter(patch => !patch.path.endsWith("live-design-refresh.patch"));
    await writeFile(path.join(stage.directory, "jarvis-stage.json"), JSON.stringify({ sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: stage.target, patches }));
    await expect(publishImpeccableNative(stage, destination)).rejects.toThrow("não corresponde");
    await expect(stat(destination)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("rejects an engine receipt missing canonical product support while retaining design support", async () => {
    const { stage, destination } = await fixture();
    const patches = (await nativeEnginePatches()).filter(patch => !patch.path.endsWith("canonical-product.patch"));
    await writeFile(path.join(stage.directory, "jarvis-stage.json"), JSON.stringify({ sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: stage.target, patches }));
    await expect(publishImpeccableNative(stage, destination)).rejects.toThrow("não corresponde");
    await expect(stat(destination)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("rejects stale patch provenance before creating or replacing any destination", async () => {
    const { stage, destination } = await fixture();
    await writeFile(path.join(stage.directory, "jarvis-stage.json"), JSON.stringify({ sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: stage.target, patches: [{ path: "scripts/impeccable-native/queue-cas.patch", sha256: "0".repeat(64) }] }));
    await expect(publishImpeccableNative(stage, destination)).rejects.toThrow("não corresponde");
    await expect(stat(destination)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("rejects a different architecture or binary outside the prepared output", async () => {
    const { stage, destination } = await fixture();
    await expect(publishImpeccableNative({ ...stage, triple: "x86_64-apple-darwin" }, destination)).rejects.toThrow("plataforma");
    await expect(publishImpeccableNative({ ...stage, binary: path.join(stage.directory, "unrelated") }, destination)).rejects.toThrow("staging");
    await expect(stat(destination)).rejects.toMatchObject({ code: "ENOENT" });
  });
});
