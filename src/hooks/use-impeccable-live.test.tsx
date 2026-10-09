import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { chatOptions } from "@/test/chat-fixtures";
import { useImpeccableLive } from "./use-impeccable-live";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
const listeners = new Set<EventCallback<unknown>>();
const status = (conversationId = "c1", state: "off" | "ready" | "setup" = "off") => ({ conversationId, state, url: state === "ready" ? "http://localhost:5173/" : null, tabId: state === "ready" ? "page-1" : null, error: null, setupNeeded: state === "setup" ? { config: "missing" } : null });
beforeEach(() => {
  vi.resetAllMocks(); listeners.clear();
  vi.mocked(listen).mockImplementation(async (_event, handler) => { listeners.add(handler); return () => { listeners.delete(handler); }; });
  vi.mocked(invoke).mockImplementation(async (command, args) => status((args as { conversationId: string }).conversationId, command === "start_impeccable_live" ? "ready" : "off"));
});

it("starts the native session for this conversation and selected model, then stops only that session", async () => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  await act(async () => { await hook.result.current.toggle(chatOptions, "http://localhost:5173/"); });
  expect(invoke).toHaveBeenCalledWith("start_impeccable_live", { conversationId: "c1", options: chatOptions, url: "http://localhost:5173/" });
  expect(hook.result.current.active).toBe(true);
  expect(hook.result.current.status.tabId).toBe("page-1");
  await act(async () => { await hook.result.current.toggle(); });
  expect(invoke).toHaveBeenCalledWith("stop_impeccable_live", { conversationId: "c1" });
  expect(hook.result.current.active).toBe(false);
  hook.unmount();
  await waitFor(() => expect(listeners.size).toBe(0));
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "stop_impeccable_live")).toHaveLength(1);
});

it("keeps setup and pumping native, ignores other conversations, and never passes a remote page as the Live target", async () => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  vi.mocked(invoke).mockImplementationOnce(async () => status("c1", "setup"));
  await act(async () => { await hook.result.current.toggle(chatOptions, "https://example.com"); });
  expect(invoke).toHaveBeenLastCalledWith("start_impeccable_live", { conversationId: "c1", options: chatOptions });
  expect(hook.result.current.status.state).toBe("setup");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "start_agent_turn")).toBe(false);
  await act(async () => { listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: status("c2", "ready") })); });
  expect(hook.result.current.status.state).toBe("setup");
  await act(async () => { listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: status("c1", "ready") })); });
  expect(hook.result.current.status.state).toBe("ready");
});

it("retains a running session after a failed stop and prevents simultaneous button requests", async () => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  await act(async () => { await hook.result.current.toggle(chatOptions); });
  vi.mocked(invoke).mockRejectedValueOnce(new Error("Cleanup unavailable"));
  await act(async () => { await Promise.all([hook.result.current.toggle(), hook.result.current.toggle()]); });
  expect(hook.result.current.active).toBe(true);
  expect(hook.result.current.busy).toBe(false);
  expect(toast.error).toHaveBeenCalledWith("Cleanup unavailable");
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "stop_impeccable_live")).toHaveLength(1);
});

it("does not overwrite a newer native event with a delayed initial read", async () => {
  let resolve: (value: unknown) => void = () => {};
  vi.mocked(invoke).mockImplementationOnce(() => new Promise(done => { resolve = done; }));
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(listeners.size).toBe(1));
  await act(async () => { listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: status("c1", "ready") })); resolve(status()); });
  expect(hook.result.current.active).toBe(true);
});

it.each(["ready", "setup"] as const)("keeps a newer %s event when the start response arrives late", async state => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  let resolve: (value: unknown) => void = () => {};
  vi.mocked(invoke).mockImplementationOnce(() => new Promise(done => { resolve = done; }));
  let toggle: Promise<void>;
  await act(async () => { toggle = hook.result.current.toggle(chatOptions); });
  await act(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: status("c1", state) }));
    resolve(status("c1", state === "ready" ? "setup" : "ready"));
    await toggle;
  });
  expect(hook.result.current.status.state).toBe(state);
  expect(hook.result.current.busy).toBe(false);
});
