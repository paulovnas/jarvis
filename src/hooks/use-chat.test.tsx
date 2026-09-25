import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import type { ChatSnapshot } from "@/core/chat";
import { useChat } from "./use-chat";
import { toast } from "sonner";
import { clearChatStore, updateChatSnapshot } from "@/core/chat-store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const listeners = new Map<string, Set<EventCallback<unknown>>>();
const running = (): ChatSnapshot => ({ ...emptyChat(), revision: 10, history: { start: 0, total: 1 }, activeTurnId: "turn1", turns: [{ ...savedTurn(), status: "running", steps: [] }] });
const completed = (): ChatSnapshot => ({ ...emptyChat(), revision: 12, history: { start: 0, total: 1 }, turns: [savedTurn()] });
const observed = (): ChatSnapshot => ({
  ...running(),
  revision: 11,
  turns: [{
    ...savedTurn(),
    status: "running",
    steps: [{ ...savedTurn().steps[0], text: "Vou conferir a configuração agora.", summary: "Analisando o ambiente", tools: [] }],
  }],
});
beforeEach(() => {
  clearChatStore();
  listeners.clear(); call.mockReset().mockResolvedValue(running());
  vi.mocked(listen).mockImplementation(async (name, callback) => {
    const set = listeners.get(name) ?? new Set(); set.add(callback); listeners.set(name, set);
    return () => { set.delete(callback); };
  });
});
async function emit(name: string, payload: unknown = null) {
  await act(async () => { listeners.get(name)?.forEach(handler => handler({ event: name, id: 1, payload })); });
}

it("acknowledges a paused question and keeps its snapshot reactive", async () => {
  const question = { turnId:"turn1", toolId:"ask-1", deadlineAt:Date.now()+30_000, questions:[{id:"q",question:"Qual opção?",options:[{label:"Uma",recommended:true}]}] };
  call.mockResolvedValue({ ...running(), pendingQuestion:question });
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.pendingQuestion).toEqual(question));
  call.mockResolvedValue({ ...running(), revision:11, pendingQuestion:{ ...question, deadlineAt:undefined } });
  await act(async () => { expect(await result.current.pauseQuestion(question)).toBe(true); });
  expect(call).toHaveBeenCalledWith("pause_agent_question", {conversationId:"c1",turnId:"turn1",toolId:"ask-1"});
  expect(result.current.snapshot?.pendingQuestion?.deadlineAt).toBeUndefined();
  expect(result.current.snapshot?.activeTurnId).toBe("turn1");
});
function update(beforeRevision: number, next: ChatSnapshot) {
  const turn = next.turns[next.turns.length - 1];
  return {
    conversationId: next.conversationId,
    baseRevision: beforeRevision,
    revision: next.revision,
    events: [
      ...(turn ? [{ type: "turnStarted", turn }] : []),
      {
        type: "stateChanged",
        state: {
          compacting: next.compacting ?? false,
          activeTurnId: next.activeTurnId,
          pendingApproval: next.pendingApproval,
          pendingQuestion: next.pendingQuestion ?? null,
          pendingAuthoring: next.pendingAuthoring ?? null,
          queuedMessages: next.queuedMessages ?? [],
          context: next.context ?? { tokens: 0, limit: null, estimated: true, compacting: false, compactions: 0 },
          compactions: next.compactions ?? [],
          fileChanges: next.fileChanges ?? [],
          history: next.history ?? { start: 0, total: next.turns.length },
        },
      },
    ],
  };
}

it("reports provider failures from execution events through Sonner without repeating them", async () => {
  const notice = vi.spyOn(toast, "error"); const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  const failure: ChatSnapshot = { ...completed(), turns: [{ ...savedTurn(), status: "error", error: { code: "account_missing", message: "O provedor foi removido." } }] };
  await emit("agent:event", update(10, failure)); await emit("agent:event", update(10, failure));
  expect(notice).toHaveBeenCalledTimes(1);
  expect(notice).toHaveBeenCalledWith("O modelo da conversa está indisponível", expect.objectContaining({ description: "O provedor foi removido." }));
});

it("recovers a missed completion when the native window regains focus", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.activeTurnId).toBe("turn1"));
  call.mockResolvedValue(completed());
  await emit("tauri://focus");
  await waitFor(() => expect(result.current.snapshot?.activeTurnId).toBeNull());
  expect(result.current.snapshot?.turns[0].steps[0].text).toBe("O projeto usa **Tauri**.");
  await emit("agent:event", update(12, running()));
  expect(result.current.snapshot?.turns[0].status).toBe("completed");
});

it("releases a crashed turn, preserves its progress and accepts the next message", async () => {
  call.mockResolvedValueOnce(observed());
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.activeTurnId).toBe("turn1"));
  const failure: ChatSnapshot = {
    ...observed(),
    revision: 12,
    activeTurnId: null,
    turns: [{
      ...observed().turns[0],
      status: "error",
      error: { code: "internal", message: "Não foi possível concluir a execução do agente." },
    }],
  };

  await emit("agent:event", update(11, failure));

  expect(result.current.snapshot?.activeTurnId).toBeNull();
  expect(result.current.snapshot?.turns[0].error?.message).toBe("Não foi possível concluir a execução do agente.");
  expect(result.current.snapshot?.turns[0].steps[0].text).toBe("Vou conferir a configuração agora.");
  const next: ChatSnapshot = {
    ...failure,
    revision: 13,
    activeTurnId: "turn2",
    history: { start: 0, total: 2 },
    turns: [...failure.turns, { ...savedTurn(), id: "turn2", user: "Continue", status: "running", steps: [] }],
  };
  call.mockResolvedValueOnce(next);
  await act(async () => {
    expect(await result.current.send("Continue", savedTurn().options)).toBe(true);
  });
  expect(result.current.snapshot?.activeTurnId).toBe("turn2");
  expect(result.current.snapshot?.turns[0]).toMatchObject(failure.turns[0]);
});

it("refreshes on browser focus and discards the result after switching projects", async () => {
  const { result, rerender, unmount } = renderHook(({ id }) => useChat(id), { initialProps: { id: "c1" } });
  await waitFor(() => expect(result.current.snapshot?.activeTurnId).toBe("turn1"));
  let resolve!: (value: unknown) => void;
  call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(call).toHaveBeenCalledTimes(2);
  call.mockResolvedValue(emptyChat("c2"));
  rerender({ id: "c2" });
  await waitFor(() => expect(result.current.snapshot?.conversationId).toBe("c2"));
  await act(async () => resolve(completed()));
  expect(result.current.snapshot?.conversationId).toBe("c2");
  unmount();
  expect([...listeners.values()].every(set => set.size === 0)).toBe(true);
});

it("does not let a delayed initial load replace a newer streaming update", async () => {
  let resolve!: (value: unknown) => void;
  call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(call).toHaveBeenCalledOnce());
  call.mockResolvedValueOnce(completed());
  await emit("agent:event", update(10, completed()));
  await act(async () => resolve(running()));
  await waitFor(() => expect(result.current.snapshot?.turns[0].status).toBe("completed"));
});

it("resyncs a matching conversation when an event batch is not understood", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.revision).toBe(10));
  call.mockResolvedValueOnce(observed());

  await emit("agent:event", { conversationId: "c1", revision: 11, events: [{ type: "futureEvent" }] });

  await waitFor(() => expect(result.current.snapshot?.turns[0].steps[0].text).toBe("Vou conferir a configuração agora."));
  expect(call).toHaveBeenLastCalledWith("subscribe_chat", { conversationId: "c1", cursor: 10 });
});

it("resyncs when an event revision reveals a missed batch", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.revision).toBe(10));
  call.mockResolvedValueOnce(observed());

  await emit("agent:event", update(8, observed()));

  await waitFor(() => expect(result.current.snapshot?.revision).toBe(11));
  expect(result.current.snapshot?.turns[0].steps[0].text).toBe("Vou conferir a configuração agora.");
});

it("catches up from the cached cursor without replacing the transcript", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.revision).toBe(10));
  call.mockResolvedValueOnce({
    protocolVersion: 1,
    reset: false,
    snapshot: null,
    batches: [update(10, observed())],
  });

  await act(async () => { window.dispatchEvent(new Event("focus")); });

  await waitFor(() => expect(result.current.snapshot?.revision).toBe(11));
  expect(result.current.snapshot?.turns[0].steps[0].text).toBe("Vou conferir a configuração agora.");
  expect(call).toHaveBeenLastCalledWith("subscribe_chat", { conversationId: "c1", cursor: 10 });
});

it("recovers an inconsistent cached page from a full subscription on reopen", async () => {
  const correct = { ...completed(), turns: [savedTurn(), { ...savedTurn(), id: "turn2", user: "Só testando de novo" }], history: { start: 7, total: 9 } };
  updateChatSnapshot("c1", () => ({ ...correct, history: { start: 8, total: 9 } }));
  call.mockImplementation(async (_command, args) => ({
    protocolVersion: 2,
    reset: false,
    snapshot: args && "cursor" in args ? null : correct,
    batches: [],
  }));

  const { result } = renderHook(() => useChat("c1"));

  await waitFor(() => expect(result.current.snapshot?.history).toEqual(correct.history));
  expect(result.current.snapshot?.turns.map(turn => turn.user)).toEqual(["Leia o README", "Só testando de novo"]);
  expect(call).toHaveBeenCalledWith("subscribe_chat", { conversationId: "c1" });
});

it("recovers a completion that happened while the chat UI was unmounted", async () => {
  const first = renderHook(() => useChat("c1"));
  await waitFor(() => expect(first.result.current.snapshot?.activeTurnId).toBe("turn1"));
  first.unmount();

  const finished = { ...completed(), revision: 11 };
  call.mockResolvedValueOnce({
    protocolVersion: 1,
    reset: false,
    snapshot: null,
    batches: [update(10, finished)],
  });
  const second = renderHook(() => useChat("c1"));

  await waitFor(() => expect(second.result.current.snapshot?.turns[0].status).toBe("completed"));
  expect(second.result.current.snapshot?.revision).toBe(11);
  expect(call).toHaveBeenLastCalledWith("subscribe_chat", { conversationId: "c1", cursor: 10 });
  expect(call).toHaveBeenCalledTimes(2);
});

it("uses workflow changes as a coalesced fallback for the main transcript", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.revision).toBe(10));
  call.mockResolvedValueOnce(observed());

  await emit("workflow:changed", { conversationId: "c1" });

  await waitFor(() => expect(result.current.snapshot?.turns[0].steps[0].text).toBe("Vou conferir a configuração agora."));
});

it("uses durable library changes when a provider streaming event is missed", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.revision).toBe(10));
  call.mockResolvedValueOnce(completed());

  await emit("library:changed", "c1");

  await waitFor(() => expect(result.current.snapshot?.turns[0].status).toBe("completed"));
  expect(call).toHaveBeenLastCalledWith("subscribe_chat", { conversationId: "c1", cursor: 10 });
});

it("keeps the loaded transcript visible when a background reconciliation fails", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.revision).toBe(10));
  call.mockRejectedValueOnce(new Error("temporary read failure"));

  await emit("library:changed", "c1");
  await waitFor(() => expect(call).toHaveBeenCalledTimes(2));

  expect(result.current.error).toBeNull();
  expect(result.current.snapshot?.turns[0].user).toBe("Leia o README");
});

it("ignores invalidation events from another conversation", async () => {
  renderHook(() => useChat("c1"));
  await waitFor(() => expect(call).toHaveBeenCalledTimes(1));
  call.mockClear();

  await emit("agent:event", { conversationId: "c2", revision: 11, events: [{ type: "futureEvent" }] });
  await emit("workflow:changed", { conversationId: "c2" });
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 150)); });

  expect(call).not.toHaveBeenCalled();
});

it("retries the failed turn in place and accepts the resumed snapshot", async () => {
  const failed: ChatSnapshot = {
    ...completed(),
    revision: 20,
    turns: [{
      ...savedTurn(),
      status: "error",
      error: { code: "provider_retry_exhausted", message: "A conexão com o provedor falhou." },
    }],
  };
  const resumed: ChatSnapshot = {
    ...failed,
    revision: 21,
    activeTurnId: "turn1",
    turns: [{ ...failed.turns[0], status: "running", error: null }],
  };
  let finishRetry!: (value: ChatSnapshot) => void;
  const pendingRetry = new Promise<ChatSnapshot>(resolve => { finishRetry = resolve; });
  call.mockImplementation(async command => command === "retry_agent_turn" ? pendingRetry : failed);
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.turns[0].status).toBe("error"));

  let first!: Promise<boolean>;
  let duplicate!: Promise<boolean>;
  act(() => {
    first = result.current.retryTurn("turn1");
    duplicate = result.current.retryTurn("turn1");
  });
  await expect(duplicate).resolves.toBe(false);
  expect(call.mock.calls.filter(([command]) => command === "retry_agent_turn")).toHaveLength(1);
  let retried = false;
  await act(async () => {
    finishRetry(resumed);
    retried = await first;
  });

  expect(retried).toBe(true);
  expect(call).toHaveBeenCalledWith("retry_agent_turn", { conversationId: "c1", turnId: "turn1" });
  expect(result.current.snapshot?.activeTurnId).toBe("turn1");
  expect(result.current.snapshot?.turns[0].status).toBe("running");
});
