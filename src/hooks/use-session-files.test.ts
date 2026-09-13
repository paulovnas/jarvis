import { act, fireEvent, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { useSessionFiles } from "./use-session-files";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const listeners = new Set<EventCallback<unknown>>();
const file = { path: "a.txt", additions: 1, deletions: 0, base: "conversation" };
beforeEach(() => {
  listeners.clear();
  vi.mocked(listen).mockImplementation(async (_event, callback) => {
    listeners.add(callback);
    return () => { listeners.delete(callback); };
  });
});
afterEach(() => { vi.useRealTimers(); call.mockReset(); vi.mocked(listen).mockReset(); });

describe("Live session files", () => {
  it("refreshes after external commits on focus and periodically without chat activity", async () => {
    call.mockResolvedValue([file]);
    const { result } = renderHook(() => useSessionFiles("c1"));
    await waitFor(() => expect(result.current.files).toHaveLength(1));
    call.mockResolvedValue([]);
    fireEvent(window, new Event("focus"));
    await waitFor(() => expect(result.current.files).toHaveLength(0));
    vi.useFakeTimers();
    const periodic = renderHook(() => useSessionFiles("c2"));
    await act(async () => {});
    call.mockResolvedValue([file]);
    await act(async () => { vi.advanceTimersByTime(10_000); });
    expect(periodic.result.current.files).toHaveLength(1);
  });
  it("clears stale files immediately and ignores late responses after changing sessions", async () => {
    let resolve!: (value: unknown) => void;
    call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    const { result, rerender } = renderHook(({ id }) => useSessionFiles(id), { initialProps: { id: "c1" } });
    call.mockResolvedValue([]);
    rerender({ id: "c2" });
    expect(result.current.files).toEqual([]);
    await act(async () => resolve([file]));
    expect(result.current.files).toEqual([]);
    expect(call).toHaveBeenCalledWith("get_agent_file_changes", { conversationId: "c2" });
  });
  it("does not retain misleading counts when live verification fails", async () => {
    call.mockResolvedValue([file]);
    const { result } = renderHook(() => useSessionFiles("c1"));
    await waitFor(() => expect(result.current.files).toHaveLength(1));
    call.mockRejectedValue({ message: "Git indisponível" });
    fireEvent(window, new Event("focus"));
    await waitFor(() => expect(result.current.error).toBe("Git indisponível"));
    expect(result.current.files).toEqual([]);
  });
  it("applies file changes from the incremental agent protocol without a second disk query", async () => {
    call.mockResolvedValue([]);
    const { result } = renderHook(() => useSessionFiles("c1"));
    await waitFor(() => expect(call).toHaveBeenCalledOnce());
    const payload = {
      conversationId: "c1",
      baseRevision: 1,
      revision: 2,
      events: [{
        type: "stateChanged",
        state: {
          compacting: false,
          activeTurnId: "turn",
          pendingApproval: null,
          pendingQuestion: null,
          pendingAuthoring: null,
          queuedMessages: [],
          context: { tokens: 0, limit: null, estimated: true, compacting: false, compactions: 0 },
          compactions: [],
          fileChanges: [file],
          history: { start: 0, total: 1 },
        },
      }],
    };
    await act(async () => {
      for (const handler of listeners) handler({ event: "agent:event", id: 1, payload });
    });
    expect(result.current.files).toEqual([file]);
    expect(call).toHaveBeenCalledOnce();
  });
});
