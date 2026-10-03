import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { useChatAgentModels } from "./use-chat-agent-models";
import type { AgentModelConfig } from "./use-agent-models";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

const primary = { account: "work", model: "primary", reasoning: null };
const secondary = { account: "personal", model: "secondary", reasoning: "high" };
const listeners = new Set<(conversationId: string) => void>();
const bindingListeners = new Set<() => void>();
let stored: Record<string, AgentModelConfig>;

function deferred<T>() {
  let resolve: (value: T) => void = () => {};
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetAllMocks(); listeners.clear(); bindingListeners.clear();
  stored = { "project-a-chat": {}, "project-b-chat": {} };
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    if (event === "provider-model-bindings:changed") {
      const callback = () => handler({ event, id: 1, payload: undefined });
      bindingListeners.add(callback);
      return () => { bindingListeners.delete(callback); };
    }
    expect(event).toBe("chat-agent-models:changed");
    const callback = (conversationId: string) => handler({ event, id: 1, payload: { conversationId } });
    listeners.add(callback);
    return () => { listeners.delete(callback); };
  });
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const payload = args as { conversationId: string; key: string; choice: typeof primary };
    if (command === "set_chat_agent_model") stored[payload.conversationId] = { ...stored[payload.conversationId], [payload.key]: payload.choice };
    return stored[payload.conversationId];
  });
});

it("persists one chat's primary and secondary without changing another chat or global agent settings", async () => {
  const first = renderHook(() => useChatAgentModels("project-a-chat"));
  const second = renderHook(() => useChatAgentModels("project-b-chat"));
  await waitFor(() => { expect(first.result.current.data).toEqual({}); expect(second.result.current.data).toEqual({}); });
  const choice = { ...primary, fallback: secondary };
  await act(async () => { expect(await first.result.current.save("standard/builder", choice)).toBe(true); });
  expect(first.result.current.data).toEqual({ "standard/builder": choice });
  expect(second.result.current.data).toEqual({});
  expect(invoke).toHaveBeenCalledWith("set_chat_agent_model", { conversationId: "project-a-chat", key: "standard/builder", choice });
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "set_agent_model")).toBe(false);
  first.unmount();
  const restored = renderHook(() => useChatAgentModels("project-a-chat"));
  await waitFor(() => expect(restored.result.current.data).toEqual({ "standard/builder": choice }));
});

it("refreshes only the conversation named by the native event and disposes its listener", async () => {
  const first = renderHook(() => useChatAgentModels("project-a-chat"));
  const second = renderHook(() => useChatAgentModels("project-b-chat"));
  await waitFor(() => { expect(first.result.current.data).not.toBeNull(); expect(second.result.current.data).not.toBeNull(); });
  vi.mocked(invoke).mockClear();
  stored["project-a-chat"] = { "standard/builder": primary };
  await act(async () => { listeners.forEach(listener => listener("project-a-chat")); });
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_chat_agent_models", { conversationId: "project-a-chat" });
  expect(first.result.current.data).toEqual({ "standard/builder": primary });
  expect(second.result.current.data).toEqual({});
  first.unmount();
  vi.mocked(invoke).mockClear();
  await act(async () => { listeners.forEach(listener => listener("project-a-chat")); });
  expect(invoke).not.toHaveBeenCalled();
});

it("ignores an old refresh that finishes after an explicit model save", async () => {
  const stale = deferred<AgentModelConfig>();
  const hook = renderHook(() => useChatAgentModels("project-a-chat"));
  await waitFor(() => expect(hook.result.current.data).toEqual({}));
  vi.mocked(invoke).mockImplementationOnce(() => stale.promise);
  let refreshing: Promise<void> = Promise.resolve();
  act(() => { refreshing = hook.result.current.refresh(); });
  await act(async () => { expect(await hook.result.current.save("standard/builder", primary)).toBe(true); });
  await act(async () => { stale.resolve({}); await refreshing; });
  expect(hook.result.current.data).toEqual({ "standard/builder": primary });
});

it("ignores a pending read from the old conversation after changing chats", async () => {
  const stale = deferred<AgentModelConfig>();
  vi.mocked(invoke).mockImplementationOnce(() => stale.promise);
  const hook = renderHook(({ id }) => useChatAgentModels(id), { initialProps: { id: "project-a-chat" } });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_chat_agent_models", { conversationId: "project-a-chat" }));
  stored["project-b-chat"] = { "designer/designer": secondary };
  hook.rerender({ id: "project-b-chat" });
  await waitFor(() => expect(hook.result.current.data).toEqual(stored["project-b-chat"]));
  await act(async () => { stale.resolve({ "standard/builder": primary }); });
  expect(hook.result.current.data).toEqual(stored["project-b-chat"]);
});

it("hides the previous chat's choices while the newly selected chat is loading", async () => {
  stored["project-a-chat"] = { "standard/builder": primary };
  const hook = renderHook(({ id }) => useChatAgentModels(id), { initialProps: { id: "project-a-chat" } });
  await waitFor(() => expect(hook.result.current.data).toEqual(stored["project-a-chat"]));
  const pending = deferred<AgentModelConfig>();
  vi.mocked(invoke).mockImplementationOnce(() => pending.promise);
  hook.rerender({ id: "project-b-chat" });
  expect(hook.result.current.data).toBeNull();
  await act(async () => { pending.resolve({ "standard/builder": secondary }); });
  expect(hook.result.current.data).toEqual({ "standard/builder": secondary });
});

it("reloads replacement choices after provider model bindings change", async () => {
  stored["project-a-chat"] = { "standard/builder": primary };
  const hook = renderHook(() => useChatAgentModels("project-a-chat"));
  await waitFor(() => expect(hook.result.current.data).toEqual(stored["project-a-chat"]));
  stored["project-a-chat"] = { "standard/builder": secondary };
  vi.mocked(invoke).mockClear();
  await act(async () => { bindingListeners.forEach(listener => listener()); });
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_chat_agent_models", { conversationId: "project-a-chat" });
  expect(hook.result.current.data).toEqual({ "standard/builder": secondary });
});

it("preserves the previous choice and releases the save lock after a persistence failure", async () => {
  stored["project-a-chat"] = { "standard/builder": primary };
  const hook = renderHook(() => useChatAgentModels("project-a-chat"));
  await waitFor(() => expect(hook.result.current.data).toEqual(stored["project-a-chat"]));
  vi.mocked(invoke).mockRejectedValueOnce({ message: "Modelo removido" });
  await act(async () => { expect(await hook.result.current.save("standard/builder", secondary)).toBe(false); });
  expect(hook.result.current.data).toEqual({ "standard/builder": primary });
  expect(hook.result.current.saving).toBe(false);
  expect(toast.error).toHaveBeenCalledWith("Modelo removido");
  await act(async () => { expect(await hook.result.current.save("standard/builder", secondary)).toBe(true); });
  expect(hook.result.current.data).toEqual({ "standard/builder": secondary });
});

it("serializes duplicate saves and shows a loading error until a refresh succeeds", async () => {
  vi.mocked(invoke).mockRejectedValueOnce({ message: "Conversa indisponível" });
  const hook = renderHook(() => useChatAgentModels("project-a-chat"));
  await waitFor(() => expect(hook.result.current.error).toBe("Conversa indisponível"));
  expect(hook.result.current.data).toBeNull();
  await act(async () => { await hook.result.current.refresh(); });
  expect(hook.result.current.error).toBeNull();
  const pending = deferred<AgentModelConfig>();
  vi.mocked(invoke).mockImplementationOnce(() => pending.promise);
  let saving: Promise<boolean> = Promise.resolve(false);
  act(() => { saving = hook.result.current.save("standard/builder", primary); });
  expect(hook.result.current.saving).toBe(true);
  await act(async () => { expect(await hook.result.current.save("standard/builder", secondary)).toBe(false); });
  await act(async () => { pending.resolve({ "standard/builder": primary }); expect(await saving).toBe(true); });
  expect(hook.result.current.saving).toBe(false);
  expect(hook.result.current.data).toEqual({ "standard/builder": primary });
});
