import { expect, it } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { populatedLibrary } from "@/test/library-fixtures";
import type { WorkflowAgent } from "@/core/workflow";
import type { RemoteChat } from "./client";
import { aggregateActivity, currentActivity } from "./remote-activity";

it("counts each scoped conversation once and keeps attention ahead of running work", () => {
  const library = { library: populatedLibrary(), runtime: [
    { conversationId: "c1", revision: 1, activeTurnId: "turn1", compacting: false, attention: [] },
    { conversationId: "c2", revision: 1, activeTurnId: "turn2", compacting: false, attention: [{ kind: "question" as const, agentId: "main" }] },
    { conversationId: "elsewhere", revision: 1, activeTurnId: null, compacting: false, attention: [], status: "failed" as const },
  ] };
  expect(aggregateActivity(library, ["c1", "c2", "c1"])).toEqual({ total: 2, running: 1, waiting: 1, failed: 0, completed: 0, status: "waiting" });
  expect(aggregateActivity(library, ["elsewhere"])).toMatchObject({ failed: 1, status: "failed" });
  expect(aggregateActivity(library, ["unloaded"])).toMatchObject({ total: 1, status: "idle" });
});

it("shows source reasoning summaries and the current tool instead of an old answer", () => {
  const turn = savedTurn(); turn.status = "running";
  turn.steps.push({ ...turn.steps[0], text: "", summary: "Já conferi o SQLite.\n\nConferindo os cenários PostgreSQL", tools: [{ ...turn.steps[0].tools[0], status: "running", name: "run_command" }] });
  const chat = { ...emptyChat(), activeTurnId: turn.id, turns: [turn] };
  expect(currentActivity({ chat, workflow: null, options: null })).toMatchObject({ status: "running", detail: "Conferindo os cenários PostgreSQL", tool: "run_command" });
});

it("prioritizes worker activity and pending decisions while the root has old commentary", () => {
  const turn = savedTurn();
  const agent: WorkflowAgent = { id: "builder", parentId: "main", role: "builder", title: "Sincronização", status: "running", createdAt: 1, updatedAt: 2, startedAt: 1, durationMs: 2, currentThought: "Validando o rollback", attempts: 1, options: turn.options, beadId: null, handoff: null, error: null, activeTurnId: "worker-turn", pendingApproval: null, pendingQuestion: null };
  const bundle: RemoteChat = { chat: { ...emptyChat(), activeTurnId: turn.id, turns: [turn] }, options: null, workflow: { conversationId: "c1", revision: 2, flow: "planned", agents: [agent] } };
  expect(currentActivity(bundle)).toMatchObject({ detail: "Validando o rollback", agent: "Construtor", status: "running" });
  agent.pendingQuestion = { turnId: "worker-turn", toolId: "question", questions: [] };
  expect(currentActivity(bundle)).toMatchObject({ status: "waiting", label: "Precisa de você" });
  agent.pendingQuestion = null; agent.status = "failed"; agent.error = "Provedor indisponível";
  agent.updatedAt = turn.createdAt + 1; bundle.chat.activeTurnId = null;
  expect(currentActivity(bundle)).toMatchObject({ status: "failed", agent: "Construtor", detail: "Provedor indisponível" });
  agent.updatedAt = turn.createdAt - 1;
  expect(currentActivity(bundle)).toMatchObject({ status: "completed" });
});
