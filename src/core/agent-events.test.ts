import { describe, expect, it } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { agentEventBatchSchema, applyAgentEventBatch } from "./agent-events";

describe("agent event protocol", () => {
  it("applies ordered deltas without replacing the complete chat snapshot", () => {
    const turn = { ...savedTurn(), status: "running" as const, steps: [{ ...savedTurn().steps[0], text: "", summary: "", tools: [] }] };
    const current = { ...emptyChat(), revision: 4, turns: [turn], activeTurnId: turn.id, history: { start: 0, total: 1 } };
    const batch = agentEventBatchSchema.parse({
      conversationId: "c1", baseRevision: 4, revision: 5,
      events: [{ type: "itemDelta", stepIndex: 0, textAppend: "Resposta", summaryAppend: "", durationMs: 25, retry: null, usage: null }],
    });
    const result = applyAgentEventBatch(current, batch);
    expect(result.needsResync).toBe(false);
    expect(result.snapshot?.turns[0].steps[0].text).toBe("Resposta");
    expect(result.snapshot?.revision).toBe(5);
  });

  it("requests a snapshot when an event does not extend the loaded revision", () => {
    const current = { ...emptyChat(), revision: 4 };
    const batch = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 2, revision: 5, events: [] });
    expect(applyAgentEventBatch(current, batch).needsResync).toBe(true);
  });

  it("ignores duplicate and stale batches without replaying their deltas", () => {
    const turn = {
      ...savedTurn(),
      status: "running" as const,
      steps: [{ ...savedTurn().steps[0], text: "Resposta", summary: "", tools: [] }],
    };
    const current = {
      ...emptyChat(),
      revision: 5,
      turns: [turn],
      activeTurnId: turn.id,
      history: { start: 0, total: 1 },
    };
    const batch = agentEventBatchSchema.parse({
      conversationId: "c1",
      baseRevision: 4,
      revision: 5,
      events: [
        {
          type: "itemDelta",
          stepIndex: 0,
          textAppend: " duplicada",
          summaryAppend: "",
          durationMs: 50,
          retry: null,
          usage: null,
        },
      ],
    });

    const result = applyAgentEventBatch(current, batch);

    expect(result.needsResync).toBe(false);
    expect(result.snapshot).toBe(current);
    expect(result.snapshot?.turns[0].steps[0].text).toBe("Resposta");
  });
});
