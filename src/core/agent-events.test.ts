import { describe, expect, it } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { readChat, type ChatSnapshot } from "./chat";
import { mergeChat } from "./chat-history";
import { agentEventBatchSchema, applyAgentEventBatch } from "./agent-events";

function history(start: number, total: number): ChatSnapshot {
  return { ...emptyChat(), revision: 4, history: { start, total }, turns: Array.from({ length: total - start }, (_, index) => ({ ...savedTurn(), id: `turn-${start + index}`, user: `Mensagem ${start + index}` })) };
}

function stateChanged(snapshot: ChatSnapshot) {
  return { type: "stateChanged", state: { ...readChat(snapshot, "c1"), pendingAuthoring: null } };
}

describe("agent event protocol", () => {
  it("preserves the loaded page coordinates when the native tail publishes state", () => {
    const current = history(3, 8);
    const tail = { ...current, revision: 5, turns: current.turns.slice(-1), history: { start: 7, total: 8 } };
    const batch = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 4, revision: 5, events: [stateChanged(tail)] });

    const applied = applyAgentEventBatch(current, batch);

    expect(applied.needsResync).toBe(false);
    expect(applied.snapshot?.history).toEqual({ start: 3, total: 8 });
    expect(applied.snapshot?.turns).toEqual(current.turns);
  });

  it.each(["event-first", "snapshot-first"])("keeps successive messages paired with their responses (%s)", order => {
    let current = history(3, 8);
    const original = current.turns;
    for (const [index, user] of ["Só testando de novo", "aaaaa"].entries()) {
      const turn = { ...savedTurn(), id: `new-${index}`, user, status: "running" as const, steps: [] };
      const revision = current.revision;
      const tail = { ...emptyChat(), revision: revision + 1, activeTurnId: turn.id, turns: [turn], history: { start: 8 + index, total: 9 + index } };
      const started = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: revision, revision: tail.revision, events: [{ type: "turnStarted", turn }, stateChanged(tail)] });
      if (order === "snapshot-first") current = mergeChat(current, tail);
      current = applyAgentEventBatch(current, started).snapshot!;
      if (order === "event-first") current = mergeChat(current, tail);
      const step = { ...savedTurn().steps[0], text: `Resposta a ${user}`, summary: "", tools: [] };
      current = applyAgentEventBatch(current, agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: tail.revision, revision: tail.revision + 1, events: [{ type: "itemStarted", item: { type: "step", stepIndex: 0, step } }] })).snapshot!;
      const completed = { ...tail, revision: tail.revision + 2, activeTurnId: null, turns: [{ ...turn, status: "completed" as const, steps: [step] }] };
      current = applyAgentEventBatch(current, agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: tail.revision + 1, revision: completed.revision, events: [{ type: "turnCompleted", turn: completed.turns[0] }, stateChanged(completed)] })).snapshot!;

      expect(current.history).toEqual({ start: 3, total: 9 + index });
      expect(current.turns.slice(0, original.length)).toEqual(original);
      expect(current.turns.slice(original.length).map(item => [item.user, item.steps[0].text])).toEqual(["Só testando de novo", "aaaaa"].slice(0, index + 1).map(message => [message, `Resposta a ${message}`]));
    }
  });

  it("does not apply the live response to the last visible turn while browsing older history", () => {
    const current = { ...history(0, 3), history: { start: 0, total: 10 }, activeTurnId: "turn-9" };
    const batch = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 4, revision: 5, events: [{ type: "itemDelta", stepIndex: 0, textAppend: "Resposta ao turno atual", summaryAppend: "", durationMs: 25, retry: null, usage: null }] });
    const applied = applyAgentEventBatch(current, batch);
    expect(applied.needsResync).toBe(false);
    expect(applied.snapshot?.turns).toEqual(current.turns);
    expect(applied.snapshot?.history).toEqual(current.history);
    expect(applied.snapshot?.revision).toBe(5);
  });

  it("keeps the page bounded when a new turn arrives at the 60-turn limit", () => {
    const current = history(20, 80);
    const turn = { ...savedTurn(), id: "new", user: "Nova mensagem" };
    const tail = { ...emptyChat(), history: { start: 80, total: 81 }, turns: [turn] };
    const batch = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 4, revision: 5, events: [{ type: "turnStarted", turn }, stateChanged(tail)] });
    const applied = applyAgentEventBatch(current, batch);
    expect(applied.snapshot?.turns).toHaveLength(60);
    expect(applied.snapshot?.turns[0].id).toBe("turn-21");
    expect(applied.snapshot?.history).toEqual({ start: 21, total: 81 });
  });

  it("shows automatic Core activity immediately and preserves it through later text deltas", () => {
    const turn = { ...savedTurn(), status: "running" as const };
    const current = { ...emptyChat(), revision: 4, turns: [turn], activeTurnId: turn.id, history: { start: 0, total: 1 } };
    const receipt = { component: "lsp", action: "post_mutation_diagnostics", status: "unavailable", summary: "Servidor indisponível", sources: ["src/app.tsx"], durationMs: 10 };
    const batch = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 4, revision: 5, events: [{ type: "itemDelta", stepIndex: 0, textAppend: "", summaryAppend: "", durationMs: 25, retry: null, usage: null, coreActivities: [receipt] }] });
    const updated = applyAgentEventBatch(current, batch).snapshot!;
    expect(updated.turns[0].steps[0].coreActivities).toEqual([receipt]);
    const next = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 5, revision: 6, events: [{ type: "itemDelta", stepIndex: 0, textAppend: "Resposta", summaryAppend: "", durationMs: 35, retry: null, usage: null }] });
    expect(applyAgentEventBatch(updated, next).snapshot?.turns[0].steps[0].coreActivities).toEqual([receipt]);
  });

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

  it("updates direct tasks before the active turn completes", () => {
    const turn = { ...savedTurn(), status: "running" as const, tasks: [] };
    const current = { ...emptyChat(), revision: 4, turns: [turn], activeTurnId: turn.id, history: { start: 0, total: 1 } };
    const tasks = [
      { id: "inspect", title: "Inspecionar o projeto", status: "completed" as const },
      { id: "implement", title: "Aplicar a correção", status: "in_progress" as const },
    ];
    const batch = agentEventBatchSchema.parse({
      protocolVersion: 3,
      conversationId: "c1",
      baseRevision: 4,
      revision: 5,
      events: [{ type: "tasksUpdated", tasks }],
    });

    const result = applyAgentEventBatch(current, batch);

    expect(result.needsResync).toBe(false);
    expect(result.snapshot?.activeTurnId).toBe(turn.id);
    expect(result.snapshot?.turns[0].status).toBe("running");
    expect(result.snapshot?.turns[0].tasks).toEqual(tasks);
  });

  it("requests a snapshot when an event does not extend the loaded revision", () => {
    const current = { ...emptyChat(), revision: 4 };
    const batch = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 2, revision: 5, events: [] });
    expect(applyAgentEventBatch(current, batch).needsResync).toBe(true);
  });

  it("requests a full snapshot if a cached page has inconsistent positions", () => {
    const current = { ...history(3, 8), history: { start: 7, total: 8 } };
    const batch = agentEventBatchSchema.parse({ conversationId: "c1", baseRevision: 4, revision: 5, events: [] });
    expect(applyAgentEventBatch(current, batch).needsResync).toBe(true);
    expect(mergeChat(current, history(3, 8)).history).toEqual({ start: 3, total: 8 });
  });

  it("requests a compatible snapshot for a newer event protocol", () => {
    const current = { ...emptyChat(), revision: 4 };
    const batch = agentEventBatchSchema.parse({ protocolVersion: 4, conversationId: "c1", baseRevision: 4, revision: 5, events: [] });
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
