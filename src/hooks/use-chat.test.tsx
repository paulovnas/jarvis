import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { useChat } from "./use-chat";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const listeners = new Map<string, Set<EventCallback<unknown>>>();
const running = () => ({ ...emptyChat(), revision: 10, history: { start: 0, total: 1 }, activeTurnId: "turn1", turns: [{ ...savedTurn(), status: "running", steps: [] }] });
const completed = () => ({ ...emptyChat(), revision: 12, history: { start: 0, total: 1 }, turns: [savedTurn()] });
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

it("recovers a missed completion when the native window regains focus", async () => {
  const { result } = renderHook(() => useChat("c1"));
  await waitFor(() => expect(result.current.snapshot?.activeTurnId).toBe("turn1"));
  call.mockResolvedValue(completed());
  await emit("tauri://focus");
  await waitFor(() => expect(result.current.snapshot?.activeTurnId).toBeNull());
  expect(result.current.snapshot?.turns[0].steps[0].text).toBe("O projeto usa **Tauri**.");
  await emit("agent:updated", running());
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
  await emit("agent:updated", completed());
  await act(async () => resolve(running()));
  expect(result.current.snapshot?.turns[0].status).toBe("completed");
});
