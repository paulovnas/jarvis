import { describe, expect, it } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { readChat } from "./chat";

describe("Chat IPC contract", () => {
  it("preserves automatic Core receipts across history reloads without requiring them in legacy data", () => {
    const turn = savedTurn();
    const receipt = { component: "open-design", action: "design_preparation", status: "reused", summary: "Referências reutilizadas", sources: ["project:DESIGN.md"], fingerprint: "digest", durationMs: 4 };
    turn.steps[0] = { ...turn.steps[0], tools: [] };
    const payload = { ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], coreActivities: [receipt] }] }] };
    expect(readChat(JSON.parse(JSON.stringify(payload)), "c1").turns[0].steps[0].coreActivities).toEqual([receipt]);
    expect(readChat({ ...emptyChat(), turns: [turn] }, "c1").turns[0].steps[0].coreActivities).toBeUndefined();
  });
  it("preserves live retry state and remains compatible with older history", () => {
    const turn = savedTurn();
    const retry = { attempt: 2, maxAttempts: 5, retryAt: 123, message: "HTTP 502" };
    const payload = { ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], retry }] }] };
    expect(readChat(payload, "c1").turns[0].steps[0].retry).toEqual(retry);
    expect(readChat({ ...emptyChat(), turns: [turn] }, "c1").turns[0].steps[0].retry).toBeUndefined();
    expect(readChat({ ...emptyChat(), turns: [turn] }, "c1").history).toEqual({ start: 0, total: 1 });
  });
  it("accepts durable real turn data and rejects another conversation or malformed tools", () => {
    expect(readChat({ ...emptyChat(), turns: [savedTurn()] }, "c1").turns[0].steps[0].tools[0].output).toBe("# Jarvis");
    expect(() => readChat(emptyChat("c2"), "c1")).toThrow();
    const turn = savedTurn();
    expect(() => readChat({ ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], tools: [{ name: "bash" }] }] }] }, "c1")).toThrow();
  });
  it("rejects snapshots from a newer incompatible protocol", () => {
    expect(() => readChat({ ...emptyChat(), protocolVersion: 4 }, "c1")).toThrow();
    expect(readChat({ ...emptyChat(), protocolVersion: 3 }, "c1").protocolVersion).toBe(3);
  });
});
