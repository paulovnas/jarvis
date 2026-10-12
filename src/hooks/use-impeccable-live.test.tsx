import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { chatOptions } from "@/test/chat-fixtures";
import { useImpeccableLive } from "./use-impeccable-live";
import type { ImpeccableLiveTask } from "./use-impeccable-live";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
const listeners = new Set<EventCallback<unknown>>();
const actionListeners = new Set<EventCallback<unknown>>();
const status = (conversationId = "c1", state: "off" | "ready" | "setup" = "off") => ({ conversationId, state, url: state === "ready" ? "http://localhost:5173/" : null, tabId: state === "ready" ? "page-1" : null, error: null, setupNeeded: state === "setup" ? { config: "missing" } : null });
beforeEach(() => {
  vi.resetAllMocks(); listeners.clear(); actionListeners.clear();
  vi.mocked(listen).mockImplementation(async (event, handler) => { const target = event === "impeccable-live:action" ? actionListeners : listeners; target.add(handler); return () => { target.delete(handler); }; });
  vi.mocked(invoke).mockImplementation(async (command, args) => status((args as { conversationId: string }).conversationId, command === "start_impeccable_live" ? "ready" : "off"));
});

it("loads saved tasks while Live is off and edits or removes only their exact revision", async () => {
  const task: ImpeccableLiveTask = { requestId: "session:17", actionId: "session", revision: 2, content: "Ajuste a versão", status: "pending", beadId: "jarvis-123", turnId: null, createdAt: 1791568800000 };
  vi.mocked(invoke).mockImplementation(async command => command === "get_impeccable_live" ? { ...status(), tasks: [task] } : { ...status(), tasks: [] });
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  expect(hook.result.current.active).toBe(false);
  expect(hook.result.current.status.tasks).toEqual([task]);
  expect(invoke).toHaveBeenCalledTimes(1);
  await act(async () => { expect(await hook.result.current.editTask(task, "Versão ao lado do nome")).toBeNull(); });
  expect(invoke).toHaveBeenLastCalledWith("edit_impeccable_live_task", { conversationId: "c1", requestId: "session:17", expectedRevision: 2, content: "Versão ao lado do nome" });
  await act(async () => { expect(await hook.result.current.removeTask(task)).toBeNull(); });
  expect(invoke).toHaveBeenLastCalledWith("remove_impeccable_live_task", { conversationId: "c1", requestId: "session:17", expectedRevision: 2 });
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "start_agent_turn" || command === "start_impeccable_live")).toBe(false);
});

it("resumes a paused task with the currently selected model and refuses unavailable recovery", async () => {
  const task: ImpeccableLiveTask = { requestId: "session:17", actionId: "session", revision: 4, content: "Ajuste a versão", status: "paused", beadId: null, turnId: null, createdAt: 1791568800000 };
  const hook = renderHook(({ options, canResume }) => useImpeccableLive("c1", true, { options, canResume }), { initialProps: { options: chatOptions, canResume: true } });
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  const options = { ...chatOptions, model: "new-model", account: "new-account" };
  hook.rerender({ options, canResume: true });
  await act(async () => { expect(await hook.result.current.resumeTask(task)).toBeNull(); });
  expect(invoke).toHaveBeenLastCalledWith("resume_impeccable_live_task", { conversationId: "c1", requestId: "session:17", expectedRevision: 4, options });
  hook.rerender({ options, canResume: false });
  const before = vi.mocked(invoke).mock.calls.length;
  await act(async () => { expect(await hook.result.current.resumeTask(task)).toContain("Selecione um modelo"); });
  expect(invoke).toHaveBeenCalledTimes(before);
});

it("uses a changed chat model for new Live requests without restarting the helper or replaying work", async () => {
  vi.mocked(invoke).mockImplementation(async () => status("c1", "ready"));
  const hook = renderHook(({ options }) => useImpeccableLive("c1", true, { options, canResume: true }), { initialProps: { options: chatOptions } });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_impeccable_live_options", { conversationId: "c1", options: chatOptions }));
  const next = { ...chatOptions, account: "other-provider", model: "other-model", reasoning: "high" };
  hook.rerender({ options: next });
  await waitFor(() => expect(invoke).toHaveBeenLastCalledWith("set_impeccable_live_options", { conversationId: "c1", options: next }));
  const calls = vi.mocked(invoke).mock.calls.length;
  hook.rerender({ options: { ...next } });
  await act(async () => { listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status("c1", "ready"), phase: "running", step: 3 } })); });
  expect(invoke).toHaveBeenCalledTimes(calls);
  expect(vi.mocked(invoke).mock.calls.some(([command]) => ["start_impeccable_live", "start_agent_turn", "resume_impeccable_live_task"].includes(command))).toBe(false);
});

it("preserves a newer task event and useful operation errors when an older RPC rejects", async () => {
  const task: ImpeccableLiveTask = { requestId: "session:17", actionId: "session", revision: 4, content: "Ajuste a versão", status: "pending", beadId: null, turnId: null, createdAt: 1791568800000 };
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  vi.mocked(invoke).mockImplementationOnce(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status(), tasks: [{ ...task, status: "running", revision: 5 }] } }));
    throw new Error("A alteração já começou e não pode mais ser editada.");
  });
  await act(async () => { expect(await hook.result.current.editTask(task, "Outro texto")).toContain("já começou"); });
  expect(hook.result.current.status.tasks[0].status).toBe("running");
  expect(hook.result.current.status.tasks[0].revision).toBe(5);
  expect(hook.result.current.busy).toBe(false);
  expect(toast.error).toHaveBeenCalledWith("A alteração já começou e não pode mais ser editada.");
});

it("scopes sidebar cancellation to the exact request when the upstream session has advanced", async () => {
  const earlier: ImpeccableLiveTask = { requestId: "session:17", actionId: "session", revision: 4, content: "Ajuste a versão", status: "running", beadId: null, turnId: "turn-17", createdAt: 1791568800000 };
  const later = { ...earlier, requestId: "session:18", revision: 1, content: "Ajuste o botão", turnId: "turn-18" };
  vi.mocked(invoke).mockImplementationOnce(async () => ({ ...status("c1", "ready"), phase: "running", actionId: "session", tasks: [earlier] }));
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  vi.mocked(invoke).mockImplementationOnce(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status("c1", "ready"), phase: "running", actionId: "session", tasks: [later] } }));
    throw new Error("Esta alteração já terminou; o pedido atual foi preservado.");
  });
  await act(async () => { expect(await hook.result.current.cancelAction(earlier.actionId, earlier.requestId, earlier.revision)).toContain("foi preservado"); });
  expect(invoke).toHaveBeenLastCalledWith("cancel_impeccable_live_action", { conversationId: "c1", actionId: "session", requestId: "session:17", expectedRevision: 4 });
  expect(hook.result.current.status.tasks).toEqual([later]);
  expect(hook.result.current.status.phase).toBe("running");
  expect(hook.result.current.busy).toBe(false);
  expect(toast.error).toHaveBeenCalledWith("Esta alteração já terminou; o pedido atual foi preservado.");
});

it("resumes a toolbar request using the latest chat selection, ignoring page-supplied options", async () => {
  vi.mocked(invoke).mockImplementationOnce(async () => ({ ...status("c1", "ready"), state: "setup", phase: "blocked", actionId: "event-1" }));
  const hook = renderHook(({ options }) => useImpeccableLive("c1", true, { options, canResume: true, tabId: "page-1", url: "http://localhost:3001/" }), { initialProps: { options: chatOptions } });
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  const options = { ...chatOptions, account: "new-account", model: "new-model" };
  hook.rerender({ options });
  await act(async () => { actionListeners.forEach(handler => handler({ event: "impeccable-live:action", id: 1, payload: { conversationId: "c1", tabId: "page-1", actionId: "event-1", action: "resume", requestId: "request-1", revision: 4, options: { model: "page-model" } } })); });
  expect(invoke).toHaveBeenCalledWith("start_impeccable_live", { conversationId: "c1", options, url: "http://localhost:3001/" });
  expect(invoke).toHaveBeenCalledWith("impeccable_live_action_feedback", { conversationId: "c1", tabId: "page-1", requestId: "request-1", error: null });
});

it("cancels only the exact Live action from its own toolbar and leaves the tab open", async () => {
  const running = { ...status("c1", "ready"), phase: "running", actionId: "event-1" };
  vi.mocked(invoke).mockImplementation(async command => command === "get_impeccable_live" ? running : { ...running, state: "setup", phase: "blocked" });
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  const request = { conversationId: "c1", tabId: "page-1", actionId: "event-1", action: "cancel", requestId: "request-1", revision: 4 };
  await act(async () => { actionListeners.forEach(handler => handler({ event: "impeccable-live:action", id: 1, payload: { ...request, actionId: "old" } })); });
  expect(invoke).toHaveBeenCalledTimes(1);
  await act(async () => { actionListeners.forEach(handler => handler({ event: "impeccable-live:action", id: 2, payload: request })); });
  expect(invoke).toHaveBeenCalledWith("cancel_impeccable_live_action", { conversationId: "c1", actionId: "event-1" });
  expect(hook.result.current.status.tabId).toBe("page-1");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "stop_impeccable_live")).toBe(false);
});

it("reports unavailable recovery selection back to the toolbar without submitting a provider turn", async () => {
  vi.mocked(invoke).mockImplementationOnce(async () => ({ ...status("c1", "ready"), state: "setup", phase: "blocked", actionId: "event-1" }));
  const hook = renderHook(() => useImpeccableLive("c1", true, { canResume: false }));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  await act(async () => { actionListeners.forEach(handler => handler({ event: "impeccable-live:action", id: 1, payload: { conversationId: "c1", tabId: "page-1", actionId: "event-1", action: "resume", requestId: "request-1", revision: 4 } })); });
  expect(invoke).toHaveBeenLastCalledWith("impeccable_live_action_feedback", { conversationId: "c1", tabId: "page-1", requestId: "request-1", error: "Selecione um modelo disponível no chat para retomar o Live." });
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "start_impeccable_live")).toBe(false);
});

it("acknowledges successful native recovery in the toolbar despite a stale RPC rejection", async () => {
  vi.mocked(invoke).mockImplementationOnce(async () => ({ ...status("c1", "ready"), state: "setup", phase: "blocked", actionId: "event-1" }));
  const hook = renderHook(() => useImpeccableLive("c1", true, { options: chatOptions, canResume: true }));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  vi.mocked(invoke).mockImplementationOnce(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: status("c1", "ready") }));
    throw new Error("Resposta anterior perdida.");
  });
  await act(async () => { actionListeners.forEach(handler => handler({ event: "impeccable-live:action", id: 1, payload: { conversationId: "c1", tabId: "page-1", actionId: "event-1", action: "resume", requestId: "request-1", revision: 4 } })); });
  expect(hook.result.current.status.state).toBe("ready");
  expect(invoke).toHaveBeenCalledWith("impeccable_live_action_feedback", { conversationId: "c1", tabId: "page-1", requestId: "request-1", error: null });
});

it("starts the native session for this conversation and selected model, then stops only that session", async () => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  await act(async () => { await hook.result.current.toggle(chatOptions, "http://localhost:5173/"); });
  expect(invoke).toHaveBeenCalledWith("start_impeccable_live", { conversationId: "c1", options: chatOptions, url: "http://localhost:5173/" });
  expect(hook.result.current.active).toBe(true);
  expect(hook.result.current.status.tabId).toBe("page-1");
  expect(hook.result.current.status.phase).toBe("idle");
  expect(hook.result.current.status.actionId).toBeNull();
  await act(async () => { await hook.result.current.toggle(); });
  expect(invoke).toHaveBeenCalledWith("stop_impeccable_live", { conversationId: "c1" });
  expect(hook.result.current.active).toBe(false);
  hook.unmount();
  await waitFor(() => expect(listeners.size).toBe(0));
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "stop_impeccable_live")).toHaveLength(1);
});

it("restores persistent blocked feedback without starting another turn and resumes with the current selection", async () => {
  vi.mocked(invoke).mockImplementationOnce(async () => ({ ...status("c1", "setup"), phase: "blocked", actionId: "event-1", error: "O Designer não recebeu esta solicitação." }));
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  expect(hook.result.current.status.error).toBe("O Designer não recebeu esta solicitação.");
  expect(hook.result.current.status.actionId).toBe("event-1");
  expect(vi.mocked(invoke).mock.calls).toHaveLength(1);
  const nextOptions = { ...chatOptions, account: "other-account", model: "other-model" };
  await act(async () => { await hook.result.current.resume(nextOptions, "http://localhost:3001/public/apresentacao"); });
  expect(invoke).toHaveBeenLastCalledWith("start_impeccable_live", { conversationId: "c1", options: nextOptions, url: "http://localhost:3001/public/apresentacao" });
  expect(hook.result.current.status.error).toBeNull();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "stop_impeccable_live" || command === "start_agent_turn")).toBe(false);
});

it("keeps failed recovery visible and ignores a remote retry target", async () => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  vi.mocked(invoke).mockRejectedValueOnce(new Error("O helper Live não respondeu."));
  await act(async () => { await hook.result.current.resume(chatOptions, "https://example.com"); });
  expect(invoke).toHaveBeenLastCalledWith("start_impeccable_live", { conversationId: "c1", options: chatOptions });
  expect(hook.result.current.status.error).toBe("O helper Live não respondeu.");
  expect(hook.result.current.status.phase).toBe("blocked");
  expect(hook.result.current.busy).toBe(false);
});

it("updates correlated waiting and running phases without submitting work from the renderer", async () => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  for (const phase of ["waiting", "running"] as const) {
    await act(async () => { listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status("c1", "ready"), phase, actionId: "event-1" } })); });
    expect(hook.result.current.status.phase).toBe(phase);
    expect(hook.result.current.status.actionId).toBe("event-1");
  }
  expect(vi.mocked(invoke).mock.calls).toHaveLength(1);
});

it.each(["config_missing", "context_missing", "target_selection_required", "page_unavailable", "dev_server_missing"])("keeps recoverable %s setup out of error notifications while the Designer prepares Live", async error => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  vi.mocked(invoke).mockImplementationOnce(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status("c1", "setup"), phase: "blocked", error: "O Designer precisa iniciar a página local.", setupNeeded: { error } } }));
    const preparing = { ...status("c1", "setup"), phase: "waiting", activity: "Preparando a página local" };
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 2, payload: preparing }));
    return preparing;
  });
  await act(async () => { await hook.result.current.toggle(chatOptions); });
  expect(hook.result.current.status.phase).toBe("waiting");
  expect(hook.result.current.status.activity).toBe("Preparando a página local");
  expect(hook.result.current.status.error).toBeNull();
  expect(toast.error).not.toHaveBeenCalled();
});

it.each(["admission_failed", "session_owned_elsewhere", "server_start_failed", "inject_failed", "event_failed"])("still reports the actual %s Live failure", async error => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  await act(async () => { listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status("c1", "setup"), phase: "blocked", error: "Não foi possível iniciar o Live.", setupNeeded: { error } } })); });
  expect(hook.result.current.status.phase).toBe("blocked");
  expect(hook.result.current.status.error).toBe("Não foi possível iniciar o Live.");
  expect(toast.error).toHaveBeenCalledWith("Modo Live", { id: "impeccable-live:c1", description: "Não foi possível iniciar o Live." });
});

it.each(["waiting", "running"] as const)("does not submit another preparation while Live setup is %s and still permits stopping it", async phase => {
  vi.mocked(invoke).mockImplementationOnce(async () => ({ ...status("c1", "setup"), phase, activity: "Preparando a página local" }));
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  await act(async () => { await hook.result.current.resume(chatOptions); });
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_impeccable_live")).toHaveLength(0);
  await act(async () => { await hook.result.current.toggle(); });
  expect(invoke).toHaveBeenLastCalledWith("stop_impeccable_live", { conversationId: "c1" });
  expect(hook.result.current.active).toBe(false);
});

it.each(["starting", "setup"] as const)("keeps an invoke failure after a native %s event actionable", async state => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  let reject: (reason: Error) => void = () => {};
  vi.mocked(invoke).mockImplementationOnce(() => new Promise((_resolve, fail) => { reject = fail; }));
  let resume: Promise<void>;
  await act(async () => { resume = hook.result.current.resume(chatOptions); });
  await act(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status("c1", "setup"), state, phase: state === "starting" ? "connecting" : "blocked", error: state === "setup" ? "Prepare a configuração Live." : null } }));
    reject(new Error("O modelo selecionado não pôde iniciar o Designer."));
    await resume;
  });
  expect(hook.result.current.status.state).toBe("setup");
  expect(hook.result.current.status.phase).toBe("blocked");
  expect(hook.result.current.status.error).toBe("O modelo selecionado não pôde iniciar o Designer.");
  expect(hook.result.current.busy).toBe(false);
  const nextOptions = { ...chatOptions, account: "other-account", model: "other-model" };
  await act(async () => { await hook.result.current.resume(nextOptions); });
  expect(invoke).toHaveBeenLastCalledWith("start_impeccable_live", { conversationId: "c1", options: nextOptions });
  expect(hook.result.current.status.state).toBe("ready");
  await act(async () => { await hook.result.current.toggle(); });
  expect(invoke).toHaveBeenLastCalledWith("stop_impeccable_live", { conversationId: "c1" });
  expect(hook.result.current.active).toBe(false);
});

it.each(["ready", "off"] as const)("preserves a newer successful native %s event after a stale invoke rejection", async state => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  let reject: (reason: Error) => void = () => {};
  vi.mocked(invoke).mockImplementationOnce(() => new Promise((_resolve, fail) => { reject = fail; }));
  let resume: Promise<void>;
  await act(async () => { resume = hook.result.current.resume(chatOptions); });
  await act(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: status("c1", state) }));
    reject(new Error("A resposta anterior foi perdida."));
    await resume;
  });
  expect(hook.result.current.status.state).toBe(state);
  expect(hook.result.current.status.error).toBeNull();
  expect(hook.result.current.status.phase).toBe("idle");
  expect(hook.result.current.busy).toBe(false);
});

it.each(["waiting", "running"] as const)("preserves newer %s setup preparation after the previous start RPC rejects", async phase => {
  const hook = renderHook(() => useImpeccableLive("c1"));
  await waitFor(() => expect(hook.result.current.loaded).toBe(true));
  vi.mocked(invoke).mockImplementationOnce(async () => {
    listeners.forEach(handler => handler({ event: "impeccable-live:changed", id: 1, payload: { ...status("c1", "setup"), phase, activity: "Preparando a página local" } }));
    throw new Error("A resposta anterior foi perdida.");
  });
  await act(async () => { await hook.result.current.resume(chatOptions); });
  expect(hook.result.current.status.state).toBe("setup");
  expect(hook.result.current.status.phase).toBe(phase);
  expect(hook.result.current.status.activity).toBe("Preparando a página local");
  expect(hook.result.current.status.error).toBeNull();
  expect(hook.result.current.busy).toBe(false);
  expect(toast.error).not.toHaveBeenCalled();
  await act(async () => { await hook.result.current.resume(chatOptions); });
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_impeccable_live")).toHaveLength(1);
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
