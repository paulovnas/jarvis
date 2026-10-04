import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { useSelfDevelopment } from "./use-self-development";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const status = (projectId: string, enabled = true) => ({ projectId, eligible: true, enabled });
const source = { id: "source", projectId: "original", projectName: "Projeto de origem", title: "Conversa de origem", status: "error" };
const incident = { id: "incident", capturedAt: 1000, conversationTitle: source.title, sourceProjectName: source.projectName, sourceStatus: "error", eventCount: 2, truncated: false, reference: "Investigue o incidente de autodesenvolvimento incident." };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.mocked(listen).mockReset().mockResolvedValue(() => {});
  call.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_self_development_status") return status((args as { projectId: string }).projectId);
    if (command === "list_self_development_sources") return [source];
    if (command === "list_self_development_incidents") return [];
    if (command === "capture_self_development_incident") return incident;
    if (command === "delete_self_development_incident") return;
    throw new Error(`Unexpected command ${command}`);
  });
});

it("clears diagnostics immediately when another window revokes the native environment", async () => {
  let changed: EventCallback<{ projectId: string }> | undefined;
  let enabled = true;
  vi.mocked(listen).mockImplementation(async (_event, handler) => { changed = handler as EventCallback<{ projectId: string }>; return () => {}; });
  call.mockImplementation(async command => {
    if (command === "get_self_development_status") return status("jarvis", enabled);
    if (command === "list_self_development_sources") return [source];
    if (command === "list_self_development_incidents") return [incident];
    throw new Error(`Unexpected command ${command}`);
  });
  const { result } = renderHook(() => useSelfDevelopment("jarvis"));
  await waitFor(() => expect(result.current.incidents).toHaveLength(1));
  enabled = false;
  act(() => changed?.({ event: "self-development-changed", id: 1, payload: { projectId: "jarvis" } }));
  expect(result.current.incidents).toEqual([]);
  expect(result.current.sources).toEqual([]);
  expect(result.current.status?.enabled).toBe(false);
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.status?.enabled).toBe(false);
});

it("discards authorization from an earlier project and fails closed for ordinary projects", async () => {
  const first = deferred<unknown>();
  call.mockImplementation(async (command, args) => {
    if (command === "get_self_development_status") return (args as { projectId: string }).projectId === "jarvis" ? first.promise : { projectId: "other", eligible: false, enabled: false };
    throw new Error(`Unexpected command ${command}`);
  });
  const view = renderHook(({ projectId }) => useSelfDevelopment(projectId), { initialProps: { projectId: "jarvis" } });
  view.rerender({ projectId: "other" });
  expect(view.result.current.status).toBeNull();
  await waitFor(() => expect(view.result.current.loading).toBe(false));
  await act(async () => { first.resolve(status("jarvis")); });
  expect(view.result.current.status).toEqual({ projectId: "other", eligible: false, enabled: false });
  expect(view.result.current.sources).toEqual([]);
  expect(view.result.current.incidents).toEqual([]);
  expect(call.mock.calls.map(([command]) => command)).toEqual(["get_self_development_status", "get_self_development_status"]);
});

it("clears loaded diagnostics on project change and ignores a late capture", async () => {
  const capture = deferred<unknown>();
  call.mockImplementation(async (command, args) => {
    if (command === "get_self_development_status") { const id = (args as { projectId: string }).projectId; return id === "jarvis" ? status(id) : { projectId: id, eligible: false, enabled: false }; }
    if (command === "list_self_development_sources") return [source];
    if (command === "list_self_development_incidents") return [incident];
    if (command === "capture_self_development_incident") return capture.promise;
    throw new Error(`Unexpected command ${command}`);
  });
  const view = renderHook(({ projectId }) => useSelfDevelopment(projectId), { initialProps: { projectId: "jarvis" } });
  await waitFor(() => expect(view.result.current.incidents).toEqual([incident]));
  let operation: Promise<unknown> | undefined;
  act(() => { operation = view.result.current.capture(source.id); });
  view.rerender({ projectId: "other" });
  expect(view.result.current.incidents).toEqual([]);
  expect(view.result.current.sources).toEqual([]);
  await act(async () => { capture.resolve(incident); await operation; });
  await waitFor(() => expect(view.result.current.status?.projectId).toBe("other"));
  expect(view.result.current.incidents).toEqual([]);
  expect(view.result.current.error).toBeNull();
});

it("does not expose diagnostics from a late list response after leaving Jarvis", async () => {
  const list = deferred<unknown>();
  call.mockImplementation(async (command, args) => {
    if (command === "get_self_development_status") { const id = (args as { projectId: string }).projectId; return id === "jarvis" ? status(id) : { projectId: id, eligible: false, enabled: false }; }
    if (command === "list_self_development_sources") return [source];
    if (command === "list_self_development_incidents") return list.promise;
    throw new Error(`Unexpected command ${command}`);
  });
  const view = renderHook(({ projectId }) => useSelfDevelopment(projectId), { initialProps: { projectId: "jarvis" } });
  await waitFor(() => expect(call).toHaveBeenCalledWith("list_self_development_incidents", { projectId: "jarvis" }));
  view.rerender({ projectId: "other" });
  await act(async () => { list.resolve([incident]); });
  expect(view.result.current.incidents).toEqual([]);
  expect(view.result.current.sources).toEqual([]);
});

it.each([
  { projectId: "different", eligible: true, enabled: true },
  { projectId: "jarvis", eligible: false, enabled: true },
  { projectId: "jarvis", eligible: "true", enabled: true },
])("denies inconsistent or malformed authorization payloads", async value => {
  call.mockResolvedValue(value);
  const { result } = renderHook(() => useSelfDevelopment("jarvis"));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.status).toBeNull();
  expect(call.mock.calls.map(([command]) => command)).toEqual(["get_self_development_status"]);
});

it("does not accept a source that was not explicitly offered by this installation", async () => {
  const { result } = renderHook(() => useSelfDevelopment("jarvis"));
  await waitFor(() => expect(result.current.loading).toBe(false));
  await act(async () => { expect(await result.current.capture("unknown")).toBeNull(); });
  expect(call).not.toHaveBeenCalledWith("capture_self_development_incident", expect.anything());
});

it("hides shared data immediately while disabling and keeps access denied on an uncertain result", async () => {
  const toggle = deferred<unknown>();
  call.mockImplementation(async command => {
    if (command === "get_self_development_status") return status("jarvis");
    if (command === "list_self_development_sources") return [source];
    if (command === "list_self_development_incidents") return [incident];
    if (command === "set_self_development_enabled") return toggle.promise;
    throw new Error(`Unexpected command ${command}`);
  });
  const { result } = renderHook(() => useSelfDevelopment("jarvis"));
  await waitFor(() => expect(result.current.incidents).toHaveLength(1));
  let operation: Promise<boolean> | undefined;
  act(() => { operation = result.current.setEnabled(false); });
  expect(result.current.status?.enabled).toBe(false);
  expect(result.current.incidents).toEqual([]);
  await act(async () => { toggle.resolve({ projectId: "another", eligible: true, enabled: false }); await operation; });
  expect(result.current.status?.enabled).toBe(false);
  expect(result.current.incidents).toEqual([]);
  expect(result.current.error).toContain("Atualize");
});
