import { describe, expect, it } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { readChat, retryStatusSchema, turnOptionsSchema } from "./chat";
import { IPC_PROTOCOL_VERSION } from "@/generated/ipc";

describe("Chat IPC contract", () => {
  it("preserves model generation measurements while retaining compatibility with older histories", () => {
    const turn = savedTurn();
    const generation = { outputTokens: 300, durationMs: 5_000, estimated: false };
    const payload = { ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], generation }] }] };
    expect(readChat(payload, "c1").turns[0].steps[0].generation).toEqual(generation);
    expect(readChat({ ...emptyChat(), turns: [turn] }, "c1").turns[0].steps[0].generation).toBeUndefined();
    expect(() => readChat({ ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], generation: { ...generation, durationMs: -1 } }] }] }, "c1")).toThrow();
  });
  it("preserves accepted primary and fallback choices through queue and history hydration", () => {
    const options = savedTurn().options;
    const modelSelection = { executor: "claude", account: "", model: "sonnet", reasoning: null, fallback: { executor: "jarvis", account: "sol", model: "gpt-6.1-sol", reasoning: "high" } };
    expect(turnOptionsSchema.parse({ ...options, modelSelection }).modelSelection).toEqual(modelSelection);
    expect(turnOptionsSchema.parse(options).modelSelection).toBeUndefined();
  });
  it.each(["pending", "issues"])("preserves the LSP %s state when reloading a conversation", (status) => {
    const turn = savedTurn();
    const receipt = { component: "lsp", action: "file_diagnostics", status, summary: "Diagnóstico por arquivo", sources: ["app.ts"], fingerprint: "current", durationMs: 1 };
    const payload = { ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], coreActivities: [receipt] }] }] };
    expect(readChat(payload, "c1").turns[0].steps[0].coreActivities).toEqual([receipt]);
  });
  it("preserves automatic Core receipts across history reloads without requiring them in legacy data", () => {
    const turn = savedTurn();
    const receipt = { component: "open-design", action: "design_preparation", status: "reused", summary: "Referências reutilizadas", sources: ["project:DESIGN.md"], fingerprint: "digest", durationMs: 4 };
    turn.steps[0] = { ...turn.steps[0], tools: [] };
    const payload = { ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], coreActivities: [receipt] }] }] };
    expect(readChat(JSON.parse(JSON.stringify(payload)), "c1").turns[0].steps[0].coreActivities).toEqual([receipt]);
    expect(readChat({ ...emptyChat(), turns: [turn] }, "c1").turns[0].steps[0].coreActivities).toBeUndefined();
  });
  it.each([[1, 1], [2, 5], [6, 8]])("preserves retry %i of %i and remains compatible with older history", (attempt, maxAttempts) => {
    const turn = savedTurn();
    const retry = { attempt, maxAttempts, retryAt: 123, message: "Reconectando a conta" };
    const payload = { ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], retry }] }] };
    expect(readChat(payload, "c1").turns[0].steps[0].retry).toEqual(retry);
    expect(readChat({ ...emptyChat(), turns: [turn] }, "c1").turns[0].steps[0].retry).toBeUndefined();
    expect(readChat({ ...emptyChat(), turns: [turn] }, "c1").history).toEqual({ start: 0, total: 1 });
  });
  it.each([[0, 1], [1, 0], [2, 1], [1.5, 5], [1, 2.5]])("rejects invalid retry %i of %i", (attempt, maxAttempts) => {
    expect(retryStatusSchema.safeParse({ attempt, maxAttempts, retryAt: 123, message: "Reconectando" }).success).toBe(false);
  });
  it("accepts durable real turn data and rejects another conversation or malformed tools", () => {
    expect(readChat({ ...emptyChat(), turns: [savedTurn()] }, "c1").turns[0].steps[0].tools[0].output).toBe("# Jarvis");
    expect(() => readChat(emptyChat("c2"), "c1")).toThrow();
    const turn = savedTurn();
    expect(() => readChat({ ...emptyChat(), turns: [{ ...turn, steps: [{ ...turn.steps[0], tools: [{ name: "bash" }] }] }] }, "c1")).toThrow();
  });
  it("rejects snapshots from a newer incompatible protocol", () => {
    expect(() => readChat({ ...emptyChat(), protocolVersion: IPC_PROTOCOL_VERSION + 1 }, "c1")).toThrow();
    expect(readChat({ ...emptyChat(), protocolVersion: 3 }, "c1").protocolVersion).toBe(3);
  });
});
