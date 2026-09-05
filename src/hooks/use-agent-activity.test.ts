import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useAgentActivity } from "./use-agent-activity";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const listeners = new Set<EventCallback<unknown>>();
const call = vi.mocked(invoke);
async function emit(conversationId: string, revision: number, activeTurnId: string | null) {
  await act(async () => { for (const handler of listeners) handler({ event: "agent:updated", id: 1, payload: { conversationId, revision, activeTurnId } }); });
}
beforeEach(() => {
  listeners.clear(); call.mockReset().mockResolvedValue([]);
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    listeners.add(handler);
    return () => { listeners.delete(handler); };
  });
});

it("restores running sessions and observes concurrent conversations without navigation dependencies", async () => {
  call.mockResolvedValue([{ conversationId: "c1", revision: 2, activeTurnId: "t1" }]);
  const { result, rerender, unmount } = renderHook(useAgentActivity);
  await waitFor(() => expect(result.current.has("c1")).toBe(true));
  await emit("c2", 3, "t2");
  expect([...result.current]).toEqual(["c1", "c2"]);
  rerender();
  await emit("c1", 5, null);
  await emit("c1", 4, "stale");
  expect([...result.current]).toEqual(["c2"]);
  await emit("c2", 4, null);
  expect(result.current.size).toBe(0);
  expect(call).toHaveBeenCalledExactlyOnceWith("get_agent_activity");
  unmount(); expect(listeners.size).toBe(0);
});

it("does not let a delayed startup snapshot overwrite newer lifecycle events", async () => {
  let resolve!: (value: unknown) => void;
  call.mockImplementation(() => new Promise(done => { resolve = done; }));
  const { result } = renderHook(useAgentActivity);
  await waitFor(() => expect(call).toHaveBeenCalled());
  await emit("c1", 4, "new-turn");
  await emit("c2", 5, null);
  await act(async () => resolve([
    { conversationId: "c1", revision: 3, activeTurnId: null },
    { conversationId: "c2", revision: 4, activeTurnId: "old-turn" },
  ]));
  expect([...result.current]).toEqual(["c1"]);
});
