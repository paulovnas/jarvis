import { expect, it } from "vitest";
import { createSnapshotWriter } from "./session-persistence";

const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve,ms));

it("serializes writes and captures queued snapshots after the earlier response finishes", async () => {
  let state = {status:"running"};
  let stored = "";
  const sent: string[] = [];
  let active = 0;
  let maximumActive = 0;
  let releaseFirst: () => void = () => undefined;
  const firstResponse = new Promise<void>((resolve) => {releaseFirst = resolve;});
  const persist = createSnapshotWriter(() => state, async (serialized) => {
    active++;
    maximumActive = Math.max(maximumActive,active);
    sent.push(serialized);
    // Without serialization, the newer response would finish before this older one.
    if (sent.length === 1) await firstResponse;
    stored = serialized;
    active--;
  });
  const first = persist();
  await pause(0);
  state = {status:"finishing"};
  const queued = persist();
  await pause(0);
  expect(sent).toEqual(['{"status":"running"}']);
  state = {status:"completed"};
  releaseFirst();
  await Promise.all([first,queued]);
  expect(maximumActive).toBe(1);
  expect(sent).toEqual(['{"status":"running"}','{"status":"completed"}']);
  expect(JSON.parse(stored)).toEqual({status:"completed"});
});

it("retries an unchanged failed snapshot and permits subsequent successful writes", async () => {
  let state = {status:"completed",revision:1};
  let attempts = 0;
  let stored = "";
  const persist = createSnapshotWriter(() => state, async (serialized) => {
    attempts++;
    if (attempts === 1) throw new Error("connection interrupted");
    stored = serialized;
  });
  await expect(persist()).rejects.toThrow("connection interrupted");
  await expect(persist()).resolves.toBeUndefined();
  expect(attempts).toBe(2);
  expect(JSON.parse(stored)).toEqual(state);
  await persist();
  expect(attempts).toBe(2);
  state = {status:"completed",revision:2};
  await persist();
  expect(attempts).toBe(3);
  expect(JSON.parse(stored)).toEqual(state);
});
