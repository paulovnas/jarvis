import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { libraryError } from "@/core/library";
import { httpDraftSchema, httpRunSchema, httpSavedRequestSchema, httpSnapshotSchema, newHttpRequest, type HttpDraft, type HttpRequest, type HttpRun, type HttpSavedRequest, type HttpSnapshot } from "@/core/http-client";

export type HttpTab = { draft: HttpDraft; request: HttpRequest; dirty: boolean; conflict: HttpDraft | null; selectedRunId: string | null };
const message = (cause: unknown) => libraryError(cause, "Não foi possível concluir a operação HTTP.");

export function mergeHttpTabs(current: HttpTab[], snapshot: HttpSnapshot): HttpTab[] {
  const incoming = new Map(snapshot.drafts.map(draft => [draft.id, draft]));
  const next = current.flatMap(tab => {
    const draft = incoming.get(tab.draft.id);
    incoming.delete(tab.draft.id);
    if (!draft) return tab.dirty ? [tab] : [];
    if (draft.revision <= tab.draft.revision) return [tab];
    return [tab.dirty ? { ...tab, conflict: draft } : { ...tab, draft, request: draft.request, conflict: null }];
  });
  for (const draft of incoming.values()) next.push({ draft, request: draft.request, dirty: false, conflict: null, selectedRunId: null });
  return next;
}

export function httpAnalysisPrompt(run: HttpRun): string {
  return `Analise a execução HTTP ${run.id} (${run.request.method} · ${run.request.name}), usando http_result para consultar esse resultado já registrado. Não reenvie a requisição. Explique o resultado e possíveis próximos passos.`;
}

// The owner mounts one controller per conversation (ConversationView key={id}).
// Cache preserves edits made immediately before navigating away; persisted drafts
// remain the source of truth for clean tabs and survive application restarts.
export function useHttpClient(conversationId: string, projectId: string, enabled = true, cache?: Map<string, HttpTab[]>) {
  const [snapshot, setSnapshot] = useState<HttpSnapshot | null>(null);
  const [tabs, setTabs] = useState<HttpTab[]>(() => cache?.get(conversationId) ?? []);
  const tabsRef = useRef(tabs);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const lock = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [closingId, setClosingId] = useState<string | null>(null);
  const generation = useRef(0);
  const mounted = useRef(true);
  const autoAttempts = useRef(new Map<string, string>());
  const invalidate = useCallback(() => { generation.current++; }, []);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; invalidate(); }; }, [invalidate]);
  const updateTabs = useCallback((update: (tabs: HttpTab[]) => HttpTab[]) => {
    tabsRef.current = update(tabsRef.current);
    if (tabsRef.current.length) cache?.set(conversationId, tabsRef.current); else cache?.delete(conversationId);
    if (mounted.current) setTabs(tabsRef.current);
  }, [cache, conversationId]);
  const refresh = useCallback(async () => {
    if (!mounted.current) return;
    const version = ++generation.current;
    const next = httpSnapshotSchema.parse(await invoke("get_http_snapshot", { conversationId }));
    if (!mounted.current || version !== generation.current) return;
    setSnapshot(next);
    updateTabs(current => mergeHttpTabs(current, next));
  }, [conversationId, updateTabs]);
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    const update = () => { void refresh().catch(cause => { if (alive) setError(message(cause)); }); };
    const subscription = listen<{ conversationId: string }>("http:changed", event => { if (event.payload.conversationId === conversationId) update(); });
    void subscription.catch(cause => { if (alive) setError(message(cause)); });
    update();
    return () => { alive = false; invalidate(); void subscription.then(stop => stop()).catch(() => {}); };
  }, [conversationId, enabled, invalidate, refresh]);

  const applySavedDraft = useCallback((draft: HttpDraft, submitted: HttpRequest) => {
    invalidate();
    updateTabs(current => {
      const tab = current.find(tab => tab.draft.id === draft.id);
      if (!tab) return [...current, { draft, request: draft.request, dirty: false, conflict: null, selectedRunId: null }];
      const changedDuringSave = JSON.stringify(tab.request) !== JSON.stringify(submitted);
      return current.map(item => item !== tab ? item : { ...tab, draft, request: changedDuringSave ? tab.request : draft.request, conflict: null, dirty: changedDuringSave });
    });
  }, [invalidate, updateTabs]);
  const persist = useCallback(async (tab: HttpTab) => {
    if (tab.conflict) throw new Error("Outra origem alterou esta requisição. Escolha a versão atual ou salve sua edição como cópia.");
    if (!tab.dirty) return tab.draft;
    const draft = httpDraftSchema.parse(await invoke("save_http_draft", { conversationId, id: tab.draft.id, revision: tab.draft.revision, request: tab.request, savedRequestId: tab.draft.savedRequestId }));
    applySavedDraft(draft, tab.request);
    return draft;
  }, [applySavedDraft, conversationId]);
  const action = useCallback(async (key: string, operation: () => Promise<void>) => {
    if (lock.current) return;
    invalidate();
    lock.current = true; setBusy(key); setError(null);
    try { await operation(); } catch (cause) { if (mounted.current) { setError(message(cause)); if (key === "open") toast.error(message(cause)); void refresh().catch(() => {}); } } finally { lock.current = false; if (mounted.current) setBusy(null); }
  }, [invalidate, refresh]);
  useEffect(() => {
    if (!enabled || !snapshot || busy) return;
    const pending = tabs.find(tab => tab.dirty && !tab.conflict && autoAttempts.current.get(tab.draft.id) !== `${tab.draft.revision}:${JSON.stringify(tab.request)}`);
    if (!pending) return;
    const timer = window.setTimeout(() => {
      autoAttempts.current.set(pending.draft.id, `${pending.draft.revision}:${JSON.stringify(pending.request)}`);
      void action("autosave", async () => { await persist(pending); });
    }, 650);
    return () => window.clearTimeout(timer);
  }, [action, busy, enabled, persist, snapshot, tabs]);
  const open = useCallback((request = newHttpRequest(), savedRequestId: string | null = null) => action("open", async () => {
    const draft = httpDraftSchema.parse(await invoke("save_http_draft", { conversationId, id: null, revision: 0, request, savedRequestId }));
    applySavedDraft(draft, request);
    if (mounted.current) setActiveId(draft.id);
  }), [action, applySavedDraft, conversationId]);
  const edit = useCallback((id: string, request: HttpRequest) => updateTabs(current => current.map(tab => tab.draft.id === id ? { ...tab, request, dirty: JSON.stringify(request) !== JSON.stringify(tab.draft.request) } : tab)), [updateTabs]);
  const saveDraft = useCallback((id: string) => action(id, async () => {
    const tab = tabsRef.current.find(item => item.draft.id === id);
    if (tab) { await persist(tab); if (mounted.current) toast.success("Rascunho salvo"); }
  }), [action, persist]);
  const saveRequest = useCallback((id: string) => action(id, async () => {
    const tab = tabsRef.current.find(item => item.draft.id === id);
    if (!tab) return;
    const draft = await persist(tab);
    const saved = snapshot?.savedRequests.find(item => item.id === draft.savedRequestId);
    const stored = httpSavedRequestSchema.parse(await invoke("save_http_request", { projectId, id: saved?.id ?? null, revision: saved?.revision ?? 0, request: draft.request }));
    const linked = httpDraftSchema.parse(await invoke("save_http_draft", { conversationId, id: draft.id, revision: draft.revision, request: draft.request, savedRequestId: stored.id }));
    applySavedDraft(linked, draft.request);
    await refresh();
    if (mounted.current) toast.success("Requisição salva no projeto");
  }), [action, applySavedDraft, conversationId, persist, projectId, refresh, snapshot?.savedRequests]);
  const deleteRequest = useCallback((saved: HttpSavedRequest) => action(saved.id, async () => {
    await invoke("delete_http_request", { projectId, id: saved.id, revision: saved.revision });
    await refresh();
    if (mounted.current) toast.success("Requisição removida do projeto");
  }), [action, projectId, refresh]);
  const send = useCallback((id: string) => action(id, async () => {
    const tab = tabsRef.current.find(item => item.draft.id === id);
    if (!tab) return;
    const draft = await persist(tab);
    const run = httpRunSchema.parse(await invoke("send_http_request", { conversationId, draftId: draft.id, revision: draft.revision }));
    updateTabs(current => current.map(item => item.draft.id === id ? { ...item, selectedRunId: run.id } : item));
    if (mounted.current) setSnapshot(current => current ? { ...current, runs: [run, ...current.runs.filter(item => item.id !== run.id)] } : current);
    await refresh();
  }), [action, conversationId, persist, refresh, updateTabs]);
  const cancel = useCallback((runId: string) => action(runId, async () => { await invoke("cancel_http_request", { conversationId, runId }); await refresh(); }), [action, conversationId, refresh]);
  const selectRun = useCallback((id: string, runId: string) => updateTabs(current => current.map(tab => tab.draft.id === id ? { ...tab, selectedRunId: runId } : tab)), [updateTabs]);
  const useRemote = useCallback((id: string) => updateTabs(current => current.map(tab => tab.draft.id === id && tab.conflict ? { ...tab, draft: tab.conflict, request: tab.conflict.request, conflict: null, dirty: false } : tab)), [updateTabs]);
  const close = useCallback((cancelRunning: boolean) => action(closingId ?? "close", async () => {
    const tab = tabsRef.current.find(item => item.draft.id === closingId);
    if (!tab) return;
    if (cancelRunning) for (const run of snapshot?.runs.filter(run => run.draftId === tab.draft.id && run.status === "running") ?? []) await invoke("cancel_http_request", { conversationId, runId: run.id });
    await invoke("close_http_draft", { conversationId, id: tab.draft.id, revision: tab.conflict?.revision ?? tab.draft.revision });
    updateTabs(current => current.filter(item => item.draft.id !== tab.draft.id));
    setActiveId(current => current === tab.draft.id ? null : current); setClosingId(null);
    await refresh();
  }), [action, closingId, conversationId, refresh, snapshot?.runs, updateTabs]);
  const openSaved = useCallback((request: HttpSavedRequest) => open(structuredClone(request.request), request.id), [open]);
  return { conversationId, projectId, snapshot, tabs, activeId, select: setActiveId, busy, error, refresh, open, openSaved, edit, saveDraft, saveRequest, deleteRequest, send, cancel, selectRun, useRemote, closingId, requestClose: setClosingId, close };
}
export type HttpClientController = ReturnType<typeof useHttpClient>;
