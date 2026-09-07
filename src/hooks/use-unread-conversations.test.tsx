import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUnreadConversations } from "./use-unread-conversations";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const handlers = new Map<string, EventCallback<unknown>>();
const notice = (conversationId: string, eventKey = "done") => ({ conversationId, eventKey });
const update = (revision: number, conversations: ReturnType<typeof notice>[]) => ({ revision, conversations });
async function emit(name: string, payload: unknown = null) {
  await act(async () => handlers.get(name)?.({ event: name, id: 1, payload }));
}
beforeEach(() => {
  handlers.clear(); vi.mocked(invoke).mockReset();
  vi.spyOn(document, "hasFocus").mockReturnValue(false);
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    handlers.set(event, handler);
    return () => { handlers.delete(event); };
  });
  vi.mocked(invoke).mockResolvedValue(update(1, [notice("a"), notice("b")]));
});
afterEach(() => { vi.restoreAllMocks(); });

describe("Unread conversations", () => {
  it("keeps background conversations unread and only reads the visible conversation on app focus", async () => {
    const { result } = renderHook(() => useUnreadConversations("a"));
    await waitFor(() => expect(result.current.size).toBe(2));
    expect(invoke).not.toHaveBeenCalledWith("mark_conversation_read", expect.anything());
    vi.mocked(invoke).mockImplementation(async command => command === "mark_conversation_read" ? update(2, [notice("b")]) : update(1, [notice("a"), notice("b")]));
    await emit("tauri://focus");
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("mark_conversation_read", { conversationId:"a", eventKey:"done" }));
    await waitFor(() => expect([...result.current]).toEqual(["b"]));
  });

  it("does not mark anything read on dashboards or when the latest transcript is not visible", async () => {
    const { result } = renderHook(() => useUnreadConversations(null));
    await waitFor(() => expect(result.current.size).toBe(2));
    await emit("tauri://focus");
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 400)); });
    expect(invoke).not.toHaveBeenCalledWith("mark_conversation_read", expect.anything());
  });

  it("cancels scheduled reads when focus or the selected conversation changes", async () => {
    const { result, rerender } = renderHook(({ id }: { id: string | null }) => useUnreadConversations(id), { initialProps: { id: "a" as string | null } });
    await waitFor(() => expect(result.current.size).toBe(2));
    await emit("tauri://focus");
    await emit("tauri://blur");
    rerender({ id: null });
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 400)); });
    expect(invoke).not.toHaveBeenCalledWith("mark_conversation_read", expect.anything());
    vi.mocked(invoke).mockImplementation(async command => command === "mark_conversation_read" ? update(2, [notice("a")]) : update(1, [notice("a"), notice("b")]));
    rerender({ id: "b" });
    await emit("tauri://focus");
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("mark_conversation_read", { conversationId:"b", eventKey:"done" }));
    expect(invoke).not.toHaveBeenCalledWith("mark_conversation_read", { conversationId:"a", eventKey:"done" });
  });

  it("ignores an older fetch arriving after a new notice", async () => {
    let resolveInitial: (value: unknown) => void = () => {};
    vi.mocked(invoke).mockImplementation(() => new Promise(resolve => { resolveInitial = resolve; }));
    const { result } = renderHook(() => useUnreadConversations(null));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_unread_conversations"));
    await emit("unread:changed", update(3, [notice("a", "new")]));
    await act(async () => resolveInitial(update(2, [])));
    expect([...result.current]).toEqual(["a"]);
    await emit("unread:changed", update(4, []));
    expect(result.current.size).toBe(0);
  });
  it("keeps a newer unread event when an older read acknowledgement arrives late", async () => {
    let resolveRead: (value: unknown) => void = () => {};
    vi.mocked(invoke).mockImplementation(async command => command === "mark_conversation_read"
      ? new Promise(resolve => { resolveRead = resolve; })
      : update(1, [notice("a")]));
    const view = renderHook(() => useUnreadConversations("a"));
    await waitFor(() => expect(view.result.current.has("a")).toBe(true));
    await emit("tauri://focus");
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("mark_conversation_read", { conversationId:"a", eventKey:"done" }));
    await emit("unread:changed", update(3, [notice("a", "new")]));
    await act(async () => resolveRead(update(2, [])));
    expect(view.result.current.has("a")).toBe(true);
    view.unmount();
  });
});
