import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import type { ChatSnapshot } from "@/core/chat";
import { useChat } from "./use-chat";
import { toast } from "sonner";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const listeners = new Map<string, Set<EventCallback<unknown>>>();
const running = (): ChatSnapshot => ({ ...emptyChat(), revision: 10, history: { start: 0, total: 1 }, activeTurnId: "turn1", turns: [{ ...savedTurn(), status: "running", steps: [] }] });
const completed = (): ChatSnapshot => ({ ...emptyChat(), revision: 12, history: { start: 0, total: 1 }, turns: [savedTurn()] });
beforeEach(() => {
  listeners.clear(); call.mockReset().mockResolvedValue(running());
  vi.mocked(listen).mockImplementation(async (name, callback) => {
    const set = listeners.get(name) ?? new Set(); set.add(callback); listeners.set(name, set);
    return () => { set.delete(callback); };
  });
});
async function emit(name: string, payload: unknown = null) {
  await act(async () => { listeners.get(name)?.forEach(handler => handler({ event: name, id: 1, payload })); });
}
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
