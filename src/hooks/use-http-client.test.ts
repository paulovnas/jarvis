import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { httpDraft, httpRun, httpSnapshot } from "@/test/http-fixtures";
import type { HttpDraft, HttpRequest, HttpSnapshot } from "@/core/http-client";
import { httpAnalysisPrompt, mergeHttpTabs, useHttpClient, type HttpTab } from "./use-http-client";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
let stored: HttpSnapshot;
beforeEach(() => {
  stored = httpSnapshot();
  mocked.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_http_snapshot") return structuredClone(stored);
    if (command === "save_http_draft") {
      const input = args as { id: string | null; revision: number; request: HttpRequest; savedRequestId: string | null };
      const draft = httpDraft({ id: input.id ?? `draft-${stored.drafts.length + 1}`, revision: input.revision + 1, request: structuredClone(input.request), savedRequestId: input.savedRequestId });
      stored.drafts = [...stored.drafts.filter(item => item.id !== draft.id), draft];
      return structuredClone(draft);
    }
    if (command === "close_http_draft") { stored.drafts = stored.drafts.filter(draft => draft.id !== (args as { id: string }).id); return; }
    if (command === "cancel_http_request") { stored.runs = stored.runs.map(run => run.id === (args as { runId: string }).runId ? { ...run, status: "cancelled", outcomeUncertain: true } : run); return; }
    throw new Error(`Unexpected command ${command}`);
  });
});
afterEach(() => vi.useRealTimers());

describe("HTTP conversation drafts", () => {
  it("preserves manual edits and the selected historical run when the agent changes a revision", () => {
    const draft = httpDraft();
    const tab: HttpTab = { draft, request: { ...draft.request, url: "https://manual.test" }, dirty: true, conflict: null, selectedRunId: "old-run" };
    const incoming = httpDraft({ revision: 2, request: { ...draft.request, url: "https://agent.test" } });
    const [merged] = mergeHttpTabs([tab], httpSnapshot({ drafts: [incoming] }));
    expect(merged.request.url).toBe("https://manual.test");
    expect(merged.selectedRunId).toBe("old-run");
    expect(merged.conflict?.revision).toBe(2);
  });

  it("accepts normalized secret references after saving without leaving plaintext or a false dirty flag", async () => {
    const { result } = renderHook(() => useHttpClient("chat-http", "project-http"));
    await waitFor(() => expect(result.current.tabs).toHaveLength(1));
    const request = { ...httpDraft().request, auth: { ...httpDraft().request.auth, type: "bearer" as const, token: "plaintext-secret" } };
    act(() => result.current.edit("draft-1", request));
    const saved = httpDraft({ revision: 2, request: { ...request, auth: { ...request.auth, token: "{{secret:credential-1}}" } } });
    mocked.mockImplementationOnce(async () => saved);
    await act(async () => result.current.saveDraft("draft-1"));
    expect(result.current.tabs[0].request.auth.token).toBe("{{secret:credential-1}}");
    expect(result.current.tabs[0].dirty).toBe(false);
  });

  it("rejects sending a conflicted draft until the user chooses a version", async () => {
    const { result } = renderHook(() => useHttpClient("chat-http", "project-http"));
    await waitFor(() => expect(result.current.tabs).toHaveLength(1));
    act(() => result.current.edit("draft-1", { ...httpDraft().request, name: "Minha edição" }));
    stored.drafts[0] = httpDraft({ revision: 2, request: { ...httpDraft().request, name: "Edição do agente" } });
    await act(async () => result.current.refresh());
    await act(async () => result.current.send("draft-1"));
    expect(result.current.error).toContain("Outra origem alterou");
    expect(result.current.tabs[0].request.name).toBe("Minha edição");
    expect(mocked.mock.calls.some(([command]) => command === "send_http_request")).toBe(false);
    act(() => result.current.useRemote("draft-1"));
    expect(result.current.tabs[0].request.name).toBe("Edição do agente");
    expect(result.current.tabs[0].conflict).toBeNull();
  });

  it("does not restore stale draft data after a newer confirmed save", async () => {
    const current = httpDraft({ revision: 3, request: { ...httpDraft().request, name: "Revisão confirmada" } });
    const tab: HttpTab = { draft: current, request: current.request, dirty: false, conflict: null, selectedRunId: null };
    expect(mergeHttpTabs([tab], httpSnapshot({ drafts: [httpDraft({ revision: 2 })] }))[0].request.name).toBe("Revisão confirmada");
  });

  it("keeps newer edits made while an older save is pending", async () => {
    const { result } = renderHook(() => useHttpClient("chat-http", "project-http"));
    await waitFor(() => expect(result.current.tabs).toHaveLength(1));
    const request = { ...httpDraft().request, name: "Primeira edição" };
    act(() => result.current.edit("draft-1", request));
    let complete!: (draft: HttpDraft) => void;
    mocked.mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }));
    let saving!: Promise<void>;
    act(() => { saving = result.current.saveDraft("draft-1"); });
    act(() => result.current.edit("draft-1", { ...request, name: "Segunda edição" }));
    await act(async () => { complete(httpDraft({ revision: 2, request })); await saving; });
    expect(result.current.tabs[0].request.name).toBe("Segunda edição");
    expect(result.current.tabs[0].draft.revision).toBe(2);
    expect(result.current.tabs[0].dirty).toBe(true);
  });

  it("autosaves partial drafts and retains pending edits across keyed chat navigation", async () => {
    const cache = new Map<string, HttpTab[]>();
    const first = renderHook(() => useHttpClient("chat-http", "project-http", true, cache));
    await waitFor(() => expect(first.result.current.tabs).toHaveLength(1));
    act(() => first.result.current.edit("draft-1", { ...httpDraft().request, name: "Rascunho incompleto", url: "{{base" }));
    first.unmount();
    const second = renderHook(() => useHttpClient("chat-http", "project-http", true, cache));
    await waitFor(() => expect(second.result.current.tabs[0]?.request.url).toBe("{{base"));
    await waitFor(() => expect(second.result.current.tabs[0].dirty).toBe(false));
    expect(stored.drafts[0].request.url).toBe("{{base");
    expect(mocked.mock.calls.some(([command]) => command === "send_http_request")).toBe(false);
  });

  it("does not let a late snapshot from an unmounted conversation affect the next controller", async () => {
    const cache = new Map<string, HttpTab[]>();
    let finish!: (snapshot: HttpSnapshot) => void;
    mocked.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const first = renderHook(() => useHttpClient("chat-http", "project-http", true, cache));
    first.unmount();
    stored = httpSnapshot({ conversationId: "second-chat", drafts: [httpDraft({ id: "other", conversationId: "second-chat" })] });
    const next = renderHook(() => useHttpClient("second-chat", "project-http", true, cache));
    await waitFor(() => expect(next.result.current.tabs[0]?.draft.id).toBe("other"));
    await act(async () => finish(httpSnapshot()));
    expect(next.result.current.tabs.map(tab => tab.draft.conversationId)).toEqual(["second-chat"]);
    expect(cache.has("chat-http")).toBe(false);
  });

  it("keeps Chat selected when runs complete in the background and never switches historical selection", async () => {
    stored.runs = [httpRun({ id: "historical" })];
    const { result } = renderHook(() => useHttpClient("chat-http", "project-http"));
    await waitFor(() => expect(result.current.tabs).toHaveLength(1));
    act(() => result.current.selectRun("draft-1", "historical"));
    stored.runs.unshift(httpRun({ id: "newest" }));
    stored.drafts.push(httpDraft({ id: "agent-created" }));
    await act(async () => result.current.refresh());
    expect(result.current.activeId).toBeNull();
    expect(result.current.tabs[0].selectedRunId).toBe("historical");
    expect(result.current.tabs.map(tab => tab.draft.id)).toContain("agent-created");
    expect(httpAnalysisPrompt(stored.runs[1])).toContain("historical");
    expect(httpAnalysisPrompt(stored.runs[1])).toContain("Não reenvie");
  });

  it.each([true, false])("closes an active tab with explicit cancellation=%s and preserves its run history", async cancelRunning => {
    stored.runs = [httpRun({ status: "running", httpStatus: null })];
    const { result } = renderHook(() => useHttpClient("chat-http", "project-http"));
    await waitFor(() => expect(result.current.tabs).toHaveLength(1));
    act(() => { result.current.select("draft-1"); result.current.requestClose("draft-1"); });
    expect(mocked.mock.calls.some(([command]) => command === "close_http_draft")).toBe(false);
    await act(async () => result.current.close(cancelRunning));
    expect(result.current.tabs).toHaveLength(0);
    expect(result.current.snapshot?.runs).toHaveLength(1);
    expect(result.current.activeId).toBeNull();
    expect(mocked.mock.calls.some(([command]) => command === "cancel_http_request")).toBe(cancelRunning);
  });
});
