import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useTerminalActivity } from "./use-terminal-activity";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const listeners = new Set<EventCallback<unknown>>();
const call = vi.mocked(invoke);

beforeEach(() => {
  listeners.clear();
  call.mockReset().mockResolvedValue([]);
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    if (event === "terminals:changed") listeners.add(handler);
    return () => { listeners.delete(handler); };
  });
});

it("restores and refreshes terminal counts for every conversation", async () => {
  call.mockResolvedValueOnce([
    { conversationId: "c1", count: 2 },
    { conversationId: "c2", count: 1 },
  ]).mockResolvedValueOnce([{ conversationId: "c2", count: 1 }]);
  const { result, unmount } = renderHook(useTerminalActivity);
  await waitFor(() => expect(result.current.get("c1")).toBe(2));
  expect(result.current.get("c2")).toBe(1);

  await act(async () => {
    for (const handler of listeners) {
      handler({ event: "terminals:changed", id: 1, payload: { conversationId: "c1" } });
    }
  });
  await waitFor(() => expect(result.current.has("c1")).toBe(false));
  expect(result.current.get("c2")).toBe(1);
  expect(call).toHaveBeenNthCalledWith(1, "get_terminal_activity");
  expect(call).toHaveBeenNthCalledWith(2, "get_terminal_activity");

  unmount();
  expect(listeners.size).toBe(0);
});
