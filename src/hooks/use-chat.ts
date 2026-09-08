import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { historyPageSchema, queuedMessageSchema, readChat, type ChatDraft, type MessagePart, type ChatSnapshot, type TurnOptions } from "@/core/chat";
import { historyWindow, mergeChat, mergeHistory, type HistoryDirection } from "@/core/chat-history";
import { libraryError } from "@/core/library";
import type { PendingQuestion, QuestionResponse } from "@/core/questions";
import { onDesktopResume } from "@/core/desktop-resume";

export function useChat(conversationId: string | null) {
  const [loaded, setLoaded] = useState<ChatSnapshot | null>(null);
  const [error, setError] = useState<{ id: string; message: string } | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [historyPending, setHistoryPending] = useState<string | null>(null);
  const [historyError, setHistoryError] = useState<{ id: string; message: string } | null>(null);
  const historyRequest = useRef(0);
  const historyLock = useRef<string | null>(null);
  const generation = useRef(0);
  const sending = useRef(false);
  const modelNotices = useRef(new Set<string>());
  const compactLocks = useRef(new Set<string>());
  const [compactingIds, setCompactingIds] = useState<ReadonlySet<string>>(() => new Set());
  const snapshot = loaded?.conversationId === conversationId ? loaded : null;

  const accept = useCallback((value: unknown, id: string) => {
    const next = readChat(value, id);
    const last = next.turns[next.turns.length - 1];
    if (last?.error && /^(account_|provider_|credential_|invalid_model|invalid_reasoning)/.test(last.error.code) && !modelNotices.current.has(last.id)) {
      modelNotices.current.add(last.id);
      toast.error("O modelo da conversa está indisponível", { id: `chat-model:${last.id}`, description: last.error.message });
    }
    setLoaded(current => mergeChat(current, next));
  }, []);

  useEffect(() => {
    const request = ++generation.current;
    if (!conversationId) return;
    let dispose: (() => void) | undefined;
    let stopResume: (() => void) | undefined;
    let active = true;
    let refreshing = false;
    let refreshAgain = false;
    const refresh = async () => {
      if (!active) return;
      if (refreshing) { refreshAgain = true; return; }
      refreshing = true;
      try {
        const value = await invoke<unknown>("get_chat", { conversationId });
        if (active) { accept(value, conversationId); setError(null); }
      } catch (cause) {
        if (active) setError({ id: conversationId, message: libraryError(cause, "Não foi possível sincronizar esta conversa.") });
      } finally {
        refreshing = false;
        if (refreshAgain && active) { refreshAgain = false; void refresh(); }
      }
    };
    // Subscribe before loading so an update cannot fall between snapshot and listener.
    void listen<unknown>("agent:updated", event => {
      if (!active) return;
      const value = event.payload;
      if (typeof value === "object" && value !== null && "conversationId" in value && value.conversationId === conversationId) {
        try { accept(value, conversationId); }
        catch { setError({ id: conversationId, message: "Uma atualização do agente não pôde ser lida. Reabra a conversa para sincronizar." }); }
      }
    }).then(unlisten => {
      if (!active) { unlisten(); return; }
      dispose = unlisten;
      stopResume = onDesktopResume(() => { void refresh(); });
      return refresh();
    }).catch((cause: unknown) => {
      if (active) setError({ id: conversationId, message: libraryError(cause, "Não foi possível abrir o histórico desta conversa.") });
    });
    return () => { active = false; dispose?.(); stopResume?.(); if (generation.current === request) generation.current += 1; };
  }, [conversationId, attempt, accept]);

  const loadHistory = async (direction: HistoryDirection): Promise<boolean> => {
    if (!conversationId || !snapshot || historyLock.current === conversationId) return false;
    const id = conversationId; const request = generation.current; const sequence = ++historyRequest.current;
    const window = historyWindow(snapshot);
    const cursor = typeof direction === "number" ? { around: direction } : direction === "older" ? { before: window.start } : direction === "newer" ? { after: window.start + snapshot.turns.length } : {};
    historyLock.current = id; setHistoryPending(id); setHistoryError(null);
    try {
      const page = historyPageSchema.parse(await invoke<unknown>("get_chat_history", { conversationId: id, ...cursor }));
      if (page.conversationId !== id) throw new Error("Mismatched history");
      if (generation.current !== request || sequence !== historyRequest.current) return false;
      setLoaded(current => current?.conversationId === id ? mergeHistory(current, page, direction) : current);
      return true;
    } catch (cause) {
      if (generation.current === request) setHistoryError({ id, message: libraryError(cause, "Não foi possível carregar este trecho.") });
      return false;
    } finally { if (sequence === historyRequest.current) { historyLock.current = null; setHistoryPending(null); } }
  };

  const send = async (content: string, options: TurnOptions, parts?: MessagePart[]): Promise<boolean> => {
    if (!conversationId || !snapshot || sending.current || compactLocks.current.has(conversationId) || snapshot.context?.compacting) return false;
    const id = conversationId; const request = generation.current;
    sending.current = true; setPendingId(id);
    try {
      const result = await invoke<unknown>("start_agent_turn", { conversationId: id, content, options, ...(parts?.length ? { parts } : {}) });
      if (generation.current === request) accept(result, id);
      return true;
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível enviar a mensagem. Seu texto foi mantido."));
      return false;
    } finally { sending.current = false; setPendingId(null); }
  };
  const stop = async () => {
    if (!snapshot?.activeTurnId) return;
    try { await invoke("cancel_agent_turn", { conversationId, turnId: snapshot.activeTurnId }); }
    catch (cause) { toast.error(libraryError(cause, "Não foi possível interromper a execução.")); }
  };
  const approve = async (approved: boolean): Promise<boolean> => {
    if (!snapshot?.activeTurnId || !snapshot.pendingApproval) return false;
    try {
      await invoke("approve_agent_tool", { conversationId, turnId: snapshot.activeTurnId, toolId: snapshot.pendingApproval.id, approved });
      return true;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível responder à autorização.")); return false; }
  };
  const removeQueued = async (messageId: string): Promise<ChatDraft | null> => {
    if (!conversationId) return null;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<{ message: unknown; snapshot: unknown }>("remove_queued_message", { conversationId: id, messageId });
      const message = queuedMessageSchema.parse(result.message);
      const next = readChat(result.snapshot, id);
      if (generation.current === request) accept(next, id);
      return { content: message.content, ...(message.parts?.length ? { parts: message.parts } : {}) };
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível retirar a mensagem da fila.")); return null; }
  };
  const answerQuestion = async (question: PendingQuestion, response: QuestionResponse): Promise<boolean> => {
    if (!conversationId || snapshot?.activeTurnId !== question.turnId || snapshot.pendingQuestion?.toolId !== question.toolId) return false;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<unknown>("answer_agent_question", { conversationId: id, turnId: question.turnId, toolId: question.toolId, response });
      if (generation.current === request) accept(result, id);
      return true;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível enviar as respostas. Tente novamente.")); return false; }
  };
  const resumeQueue = async () => {
    if (!conversationId) return;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<unknown>("resume_agent_queue", { conversationId: id });
      if (generation.current === request) accept(result, id);
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível continuar a fila.")); }
  };
  const compact = async (): Promise<boolean> => {
    if (!conversationId || !snapshot || snapshot.activeTurnId || snapshot.context?.compacting || compactLocks.current.has(conversationId) || sending.current) return false;
    const id = conversationId; const request = generation.current;
    compactLocks.current.add(id); setCompactingIds(new Set(compactLocks.current));
    try {
      const result = await invoke<unknown>("compact_agent_context", { conversationId: id });
      if (generation.current === request) accept(result, id);
      toast.success("Contexto compactado");
      return true;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível compactar o contexto.")); return false; }
    finally { compactLocks.current.delete(id); setCompactingIds(new Set(compactLocks.current)); }
  };
  return { snapshot, loadHistory, historyLoading: historyPending === conversationId && conversationId !== null, historyError: historyError?.id === conversationId ? historyError.message : null, error: error?.id === conversationId ? error.message : null, pending: pendingId === conversationId && conversationId !== null, compacting: (conversationId !== null && compactingIds.has(conversationId)) || snapshot?.context?.compacting === true, send, stop, approve, answerQuestion, removeQueued, resumeQueue, compact, retry: () => setAttempt(value => value + 1) };
}
export type ChatController = ReturnType<typeof useChat>;
