import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { applyAgentEventBatch, type AgentEventBatch } from "./agent-events";
import type { ChatSnapshot } from "./chat";
import { createAgentEventQueue } from "./agent-event-queue";

function delta(revision: number, text = "x"): AgentEventBatch {
  return { protocolVersion: 3, conversationId: "c1", baseRevision: revision - 1, revision,
    events: [{ type: "itemDelta", stepIndex: 0, textAppend: text, summaryAppend: "", durationMs: 0, retry: null, usage: null }] };
}

describe("bounded agent event dispatch", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

  function receiver(paused = false) {
    let snapshot: ChatSnapshot | null = { ...emptyChat(), revision: 0, turns: [{ ...savedTurn(), status: "running", steps: [{ ...savedTurn().steps[0], text: "" }] }], activeTurnId: "turn1" };
    const recoveries: boolean[] = [];
    const queue = createAgentEventQueue(batch => {
      const result = applyAgentEventBatch(snapshot, batch);
      snapshot = result.snapshot;
      return !result.needsResync;
    }, full => recoveries.push(full), paused);
    return { queue, recoveries, read: () => snapshot };
  }

  it("yields during a burst and eventually applies every revision in order", () => {
    const { queue, read, recoveries } = receiver();
    for (let revision = 1; revision <= 250; revision++) queue.enqueue(delta(revision));
    expect(read()?.revision).toBe(0);
    vi.advanceTimersToNextTimer();
    expect(read()?.revision).toBeGreaterThan(0);
    expect(read()?.revision).toBeLessThan(250);
    vi.runAllTimers();
    expect(read()?.revision).toBe(250);
    expect(read()?.turns[0].steps[0].text).toBe("x".repeat(250));
    expect(recoveries).toEqual([]);
    queue.dispose();
  });

  it("yields on elapsed work before reaching the event count budget", () => {
    let now = 0;
    vi.spyOn(performance, "now").mockImplementation(() => now += 5);
    const { queue, read } = receiver();
    for (let revision = 1; revision <= 20; revision++) queue.enqueue(delta(revision));
    vi.advanceTimersToNextTimer();
    expect(read()?.revision).toBeGreaterThan(0);
    expect(read()?.revision).toBeLessThan(20);
    vi.runAllTimers();
    expect(read()?.turns[0].steps[0].text).toBe("x".repeat(20));
  });

  it("pauses a revision gap and reconciles replay with queued live batches", () => {
    const { queue, read, recoveries } = receiver();
    queue.enqueue(delta(2, "B"));
    queue.enqueue(delta(3, "C"));
    vi.runAllTimers();
    expect(read()?.revision).toBe(0);
    expect(recoveries).toEqual([false]);
    queue.resume([delta(1, "A"), delta(2, "B"), delta(3, "C")]);
    vi.runAllTimers();
    expect(read()?.turns[0].steps[0].text).toBe("ABC");
    expect(read()?.revision).toBe(3);
    expect(recoveries).toEqual([false]);
  });

  it("bounds early events by count and asks for a fresh snapshot on overflow", () => {
    const { queue, read, recoveries } = receiver(true);
    for (let revision = 1; revision <= 600; revision++) queue.enqueue(delta(revision));
    vi.runAllTimers();
    expect(read()?.revision).toBe(0);
    expect(recoveries).toEqual([true]);
    queue.dispose();
  });

  it("bounds a single oversized event instead of retaining its text", () => {
    const { queue, read, recoveries } = receiver(true);
    queue.enqueue(delta(1, "x".repeat(3 * 1024 * 1024)));
    queue.resume();
    vi.runAllTimers();
    expect(read()?.revision).toBe(0);
    expect(recoveries).toEqual([true]);
  });

  it("releases pending work on disposal and ignores later arrivals", () => {
    const { queue, read } = receiver();
    queue.enqueue(delta(1));
    queue.dispose();
    queue.enqueue(delta(2));
    queue.resume();
    vi.runAllTimers();
    expect(read()?.revision).toBe(0);
  });
});
