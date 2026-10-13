import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { computerApp, computerSnapshot } from "@/test/computer-fixtures";
import { useComputerUse } from "./use-computer-use";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const listeners = new Set<EventCallback<unknown>>();
const modeListeners = new Set<EventCallback<unknown>>();

beforeEach(() => {
  listeners.clear();
  modeListeners.clear();
  call.mockReset().mockResolvedValue(computerSnapshot());
  vi.mocked(listen).mockImplementation(async (name, callback) => {
    if (name === "computer-use-updated") listeners.add(callback);
    if (name === "computer-use-mode-changed") modeListeners.add(callback);
    return () => { listeners.delete(callback); modeListeners.delete(callback); };
  });
});

async function emit(conversationId: string, snapshot: unknown) {
  await act(async () => {
    listeners.forEach(callback => callback({ event: "computer-use-updated", id: 1, payload: { conversationId, snapshot } }));
  });
}

it("refreshes the mode changed in settings and preserves it when opening a new chat", async () => {
  const { result, rerender } = renderHook(({ id }) => useComputerUse(id), { initialProps: { id: "c1" } });
  await waitFor(() => expect(result.current.snapshot?.mode).toBe("manual"));
  call.mockResolvedValue(computerSnapshot({ mode: "yolo" }));
  await act(async () => { modeListeners.forEach(callback => callback({ event: "computer-use-mode-changed", id: 1, payload: "yolo" })); });
  await waitFor(() => expect(result.current.snapshot?.mode).toBe("yolo"));
  rerender({ id: "c2" });
  await waitFor(() => expect(result.current.snapshot?.mode).toBe("yolo"));
  expect(call).toHaveBeenCalledWith("get_computer_use_status", { conversationId: "c2" });
  expect(call).not.toHaveBeenCalledWith("authorize_computer_app", expect.anything());
});

it("refreshes OS permission status when returning from System Settings", async () => {
  const missing = computerSnapshot();
  missing.capabilities.accessibility = false;
  call.mockResolvedValue(missing);
  const { result } = renderHook(() => useComputerUse("c1"));
  await waitFor(() => expect(result.current.snapshot?.capabilities.accessibility).toBe(false));
  call.mockResolvedValue(computerSnapshot());
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  await waitFor(() => expect(result.current.snapshot?.capabilities.accessibility).toBe(true));
  expect(call).not.toHaveBeenCalledWith("request_computer_permission", expect.anything());
});

it("preserves a confirmed live update when the initial status reply arrives late", async () => {
  let complete!: (value: unknown) => void;
  call.mockReturnValue(new Promise(resolve => { complete = resolve; }));
  const { result } = renderHook(() => useComputerUse("c1"));
  await waitFor(() => expect(listeners.size).toBe(1));
  await emit("c1", computerSnapshot({ stopped: true }));
  await act(async () => { complete(computerSnapshot()); });
  expect(result.current.snapshot?.stopped).toBe(true);
});

it("loads initial capabilities even when app inventory finishes first", async () => {
  let complete!: (value: unknown) => void;
  call.mockImplementation(async command => command === "get_computer_use_status"
    ? new Promise(resolve => { complete = resolve; }) : [computerApp]);
  const { result } = renderHook(() => useComputerUse("c1"));
  await act(async () => { await result.current.loadApps(); });
  expect(result.current.apps).toEqual([computerApp]);
  expect(result.current.snapshot).toBeNull();
  await act(async () => { complete(computerSnapshot()); });
  expect(result.current.snapshot?.capabilities.supported).toBe(true);
});

it("ignores another conversation and an old authorization result after switching chats", async () => {
  let complete!: (value: unknown) => void;
  call.mockImplementation(async command => command === "authorize_computer_app"
    ? new Promise(resolve => { complete = resolve; }) : computerSnapshot());
  const { result, rerender } = renderHook(({ id }) => useComputerUse(id), { initialProps: { id: "c1" } });
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  let pending!: Promise<boolean>;
  act(() => { pending = result.current.authorize("safari", "control"); });
  rerender({ id: "c2" });
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  await emit("c1", computerSnapshot({ stopped: true }));
  await act(async () => {
    complete(computerSnapshot({ grants: [{ id: "old", identity: computerApp.identity, access: "control" }] }));
    await pending;
  });
  expect(result.current.snapshot?.grants).toEqual([]);
  expect(result.current.snapshot?.stopped).toBe(false);
});

it("keeps the last confirmed state when a refreshed status is malformed", async () => {
  const { result } = renderHook(() => useComputerUse("c1"));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  call.mockResolvedValue({ capabilities: { supported: true }, grants: [] });
  await act(async () => { await result.current.refresh(); });
  expect(result.current.snapshot?.capabilities.accessibility).toBe(true);
  expect(result.current.error).toMatch(/inválido/);
});

it("reports a malformed update for this conversation without replacing confirmed access", async () => {
  const { result } = renderHook(() => useComputerUse("c1"));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  await emit("c1", { grants: [] });
  expect(result.current.error).toMatch(/inválido/);
  expect(result.current.snapshot?.capabilities.supported).toBe(true);
});

it("lets stop supersede an in-flight authorization without restoring its stale result", async () => {
  let complete!: (value: unknown) => void;
  call.mockImplementation(async command => command === "authorize_computer_app"
    ? new Promise(resolve => { complete = resolve; })
    : command === "stop_computer_use" ? computerSnapshot({ stopped: true }) : computerSnapshot());
  const { result } = renderHook(() => useComputerUse("c1"));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  let pending!: Promise<boolean>;
  act(() => { pending = result.current.authorize("safari", "control"); });
  await act(async () => { await result.current.stop(); });
  await act(async () => { complete(computerSnapshot({ grants: [{ id: "stale", identity: computerApp.identity, access: "control" }] })); await pending; });
  expect(result.current.snapshot?.stopped).toBe(true);
  expect(result.current.snapshot?.grants).toEqual([]);
});

it("does not report a late native authorization as accepted after the consumer unmounts", async () => {
  let complete!: (value: unknown) => void;
  call.mockImplementation(async command => command === "authorize_computer_app" ? new Promise(resolve => { complete = resolve; }) : computerSnapshot());
  const { result, unmount } = renderHook(() => useComputerUse("c1"));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  let pending!: Promise<boolean>;
  act(() => { pending = result.current.authorize("safari", "observe"); });
  unmount();
  complete(computerSnapshot());
  expect(await pending).toBe(false);
});
