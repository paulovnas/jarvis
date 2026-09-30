import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import type { BrowserRequest, BrowserSnapshot, BrowserTab } from "@/core/browser";
import { useBrowser } from "./use-browser";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
const listeners = new Map<string, EventCallback<unknown>>();
const first: BrowserTab = { id: "ext:epoch:1", conversationId: "chat-1", title: "Primeira página", url: "https://example.com", loading: false };
const second: BrowserTab = { ...first, id: "ext:epoch:2", title: "Segunda página" };
beforeEach(() => {
  listeners.clear();
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    listeners.set(event, handler);
    return () => { listeners.delete(event); };
  });
  mocked.mockReset().mockImplementation(async command => command === "get_browser_tabs" ? { tabs: [], activeId: null } : { completed: true });
});

async function browserChanged() {
  await act(async () => { listeners.get("browser:changed")?.({ event: "browser:changed", id: 1, payload: { conversationId: "chat-1" } }); });
}

it.each(["extension", "embedded", undefined] as const)("keeps external agent targets silent with backend %s", async backend => {
  let stored: BrowserSnapshot = { tabs: [first], activeId: first.id, backend };
  mocked.mockImplementation(async command => command === "get_browser_tabs" ? stored : { completed: true });
  const activate = vi.fn();
  const { result } = renderHook(() => useBrowser("chat-1", activate));
  await waitFor(() => expect(result.current.loaded).toBe(true));
  expect(result.current.snapshot.tabs).toEqual([first]);
  expect(result.current.snapshot.activeId).toBeNull();
  stored = { ...stored, tabs: [first, second], activeId: second.id };
  await browserChanged();
  await waitFor(() => expect(result.current.snapshot.tabs).toHaveLength(2));
  expect(result.current.snapshot.activeId).toBeNull();
  expect(activate).not.toHaveBeenCalled();
});

it("preserves explicit external selection when the agent changes targets and returns to Chat when that tab closes", async () => {
  let stored: BrowserSnapshot = { tabs: [first, second], activeId: first.id, backend: "extension" };
  mocked.mockImplementation(async command => command === "get_browser_tabs" ? stored : { completed: true });
  const activate = vi.fn();
  const { result } = renderHook(() => useBrowser("chat-1", activate));
  await waitFor(() => expect(result.current.loaded).toBe(true));
  await act(async () => { result.current.select(first.id); });
  expect(result.current.snapshot.activeId).toBe(first.id);
  stored = { ...stored, activeId: second.id };
  await browserChanged();
  expect(result.current.snapshot.activeId).toBe(first.id);
  stored = { ...stored, tabs: [second] };
  await browserChanged();
  expect(result.current.snapshot.activeId).toBeNull();
  expect(activate).toHaveBeenCalledTimes(1);
});

it.each(["open", "attach"] as const)("reveals extension tabs for explicit user %s actions, including concurrent agent events", async action => {
  let stored: BrowserSnapshot = { tabs: [], activeId: null, backend: "extension" };
  mocked.mockImplementation(async command => {
    if (command === "get_browser_tabs") return stored;
    if (command === "browser_command") {
      stored = { ...stored, tabs: [first], activeId: first.id };
      listeners.get("browser:changed")?.({ event: "browser:changed", id: 1, payload: { conversationId: "chat-1" } });
      return stored;
    }
    return { completed: true };
  });
  const activate = vi.fn();
  const { result } = renderHook(() => useBrowser("chat-1", activate));
  await waitFor(() => expect(result.current.loaded).toBe(true));
  await act(async () => { await result.current.command({ action, ...(action === "attach" ? { id: first.id } : {}) }); });
  expect(result.current.snapshot.activeId).toBe(first.id);
  expect(activate).toHaveBeenCalledTimes(1);
});

it("continues revealing embedded browser activity", async () => {
  const native = { ...first, id: "native-tab" };
  mocked.mockResolvedValue({ tabs: [native], activeId: native.id, backend: "embedded" });
  const activate = vi.fn();
  const { result } = renderHook(() => useBrowser("chat-1", activate));
  await waitFor(() => expect(result.current.snapshot.activeId).toBe(native.id));
  expect(activate).toHaveBeenCalledTimes(1);
});

it("ignores a previous conversation's delayed catalog", async () => {
  let finish!: (value: BrowserSnapshot) => void;
  const pending = new Promise<BrowserSnapshot>(resolve => { finish = resolve; });
  mocked.mockImplementation(async (command, args) => {
    if (command === "get_browser_tabs") return (args as { conversationId: string }).conversationId === "chat-1" ? pending : { tabs: [], activeId: null, backend: "extension" };
    return { completed: true };
  });
  const activate = vi.fn();
  const { result, rerender } = renderHook(({ conversationId }) => useBrowser(conversationId, activate), { initialProps: { conversationId: "chat-1" } });
  rerender({ conversationId: "chat-2" });
  await waitFor(() => expect(result.current.loaded).toBe(true));
  await act(async () => { finish({ tabs: [first], activeId: first.id, backend: "extension" }); });
  expect(result.current.snapshot.tabs).toEqual([]);
  expect(result.current.snapshot.activeId).toBeNull();
  expect(activate).not.toHaveBeenCalled();
});

it("does not reveal a pending user open after leaving the conversation", async () => {
  let finish!: (value: BrowserSnapshot) => void;
  const pending = new Promise<BrowserSnapshot>(resolve => { finish = resolve; });
  mocked.mockImplementation(async command => command === "browser_command" ? pending : { tabs: [], activeId: null, backend: "extension" });
  const activate = vi.fn();
  const { result, unmount } = renderHook(() => useBrowser("chat-1", activate));
  await waitFor(() => expect(result.current.loaded).toBe(true));
  let opening!: Promise<unknown>;
  act(() => { opening = result.current.command({ action: "open" }); });
  unmount();
  await act(async () => { finish({ tabs: [first], activeId: first.id, backend: "extension" }); await opening; });
  expect(activate).not.toHaveBeenCalled();
});

it("lets a later choice of Chat override an in-flight explicit open", async () => {
  let stored: BrowserSnapshot = { tabs: [], activeId: null, backend: "extension" };
  let finish!: (value: BrowserSnapshot) => void;
  const pending = new Promise<BrowserSnapshot>(resolve => { finish = resolve; });
  mocked.mockImplementation(async (command, args) => {
    if (command === "get_browser_tabs") return stored;
    if (command === "browser_command" && (args as { request: BrowserRequest }).request.action === "open") return pending;
    return { completed: true };
  });
  const activate = vi.fn();
  const { result } = renderHook(() => useBrowser("chat-1", activate));
  await waitFor(() => expect(result.current.loaded).toBe(true));
  let opening!: Promise<unknown>;
  act(() => { opening = result.current.command({ action: "open" }); });
  await act(async () => { result.current.select(null); });
  stored = { ...stored, tabs: [first], activeId: first.id };
  await act(async () => { finish(stored); await opening; });
  expect(result.current.snapshot.tabs).toEqual([first]);
  expect(result.current.snapshot.activeId).toBeNull();
  expect(activate).not.toHaveBeenCalled();
});

it("forwards current IDs, semantic locators, frame pagination and explicit waits to the native contract", async () => {
  const { result } = renderHook(() => useBrowser("chat-1", undefined, false));
  const requests: BrowserRequest[] = [
    { action: "click", id: "native-tab", element: "element-1" },
    { action: "click", id: "ext:epoch:7", locator: { role: "button", name: "Salvar" }, frameId: "frame-1", timeoutMs: 5000 },
    { action: "snapshot", id: "ext:epoch:7", frameId: "frame-1", offset: 10, limit: 100 },
    { action: "wait", id: "ext:epoch:7", locator: { testId: "saved", exact: true }, state: "visible", timeoutMs: 15000 },
    { action: "wait", id: "ext:epoch:7", state: "ready", timeoutMs: 0 },
  ];
  for (const request of requests) {
    await act(async () => { expect(await result.current.command(request)).toEqual({ completed: true }); });
    expect(mocked).toHaveBeenCalledWith("browser_command", { conversationId: "chat-1", request });
  }
});
