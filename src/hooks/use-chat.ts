import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { historyPageSchema, queuedMessageSchema, readChat, type AgentTurn, type ApprovalDecision, type ChatDraft, type MessagePart, type ChatSnapshot, type TurnOptions } from "@/core/chat";
import { hasValidHistoryWindow, historyWindow, mergeChat, mergeHistory, type HistoryDirection } from "@/core/chat-history";
import { libraryError } from "@/core/library";
import type { PendingQuestion, QuestionResponse } from "@/core/questions";
import { onDesktopResume } from "@/core/desktop-resume";
import type { PendingAuthoring } from "@/core/authoring";
import { agentEventBatchSchema, applyAgentEventBatch, chatSubscriptionSchema, type AgentEventBatch } from "@/core/agent-events";
import { getChatSnapshot, subscribeChatSnapshot, updateChatSnapshot } from "@/core/chat-store";
import { IPC_PROTOCOL_VERSION } from "@/generated/ipc";

function eventConversationId(payload: unknown): string | null {
  if (typeof payload === "string") return payload;
  if (!payload || typeof payload !== "object") return null;
  const id = Reflect.get(payload, "conversationId");
  return typeof id === "string" ? id : null;
}

function matchesPendingTurn(observed: AgentTurn | undefined, pending: AgentTurn): boolean {
  return Boolean(observed
    && observed.user === pending.user
    && observed.options.account === pending.options.account
    && observed.options.model === pending.options.model
    && observed.createdAt >= pending.createdAt - 5_000);
}

function observesPendingTurn(snapshot: ChatSnapshot, pending: AgentTurn): boolean {
  return matchesPendingTurn(snapshot.turns[snapshot.turns.length - 1], pending);
}

export function useChat(conversationId: string | null) {
  const [error, setError] = useState<{ id: string; message: string } | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [pendingTurn, setPendingTurn] = useState<{ conversationId: string; turn: AgentTurn } | null>(null);
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
  const subscribeStore = useCallback((listener: () => void) => subscribeChatSnapshot(conversationId, listener), [conversationId]);
  const readStore = useCallback(() => getChatSnapshot(conversationId), [conversationId]);
  const snapshot = useSyncExternalStore(subscribeStore, readStore, () => null);

  const reportModelError = useCallback((next: ChatSnapshot) => {
    const last = next.turns[next.turns.length - 1];
    if (last?.error && /^(account_|provider_|credential_|invalid_model|invalid_reasoning)/.test(last.error.code) && !modelNotices.current.has(last.id)) {
      modelNotices.current.add(last.id);
      toast.error("O modelo da conversa está indisponível", { id: `chat-model:${last.id}`, description: last.error.message });
    }
  }, []);
  const accept = useCallback((value: unknown, id: string) => {
    const next = readChat(value, id);
    reportModelError(next);
    setError(current => current?.id === id ? null : current);
    setPendingTurn(current => current?.conversationId === id && observesPendingTurn(next, current.turn) ? null : current);
    updateChatSnapshot(id, current => mergeChat(current, next));
  }, [reportModelError]);

  useEffect(() => {
    const request = ++generation.current;
    if (!conversationId) return;
    const dispose: Array<() => void> = [];
    let stopResume: (() => void) | undefined;
    let active = true;
    let hasSnapshot = getChatSnapshot(conversationId) !== null;
    let refreshing = false;
    let refreshAgain = false;
    let resyncTimer: ReturnType<typeof setTimeout> | undefined;
    const earlyBatches: AgentEventBatch[] = [];
    const applyBatch = (batch: AgentEventBatch) => {
      updateChatSnapshot(conversationId, current => {
        const applied = applyAgentEventBatch(current, batch);
        if (applied.needsResync) scheduleResync();
        if (applied.snapshot) {
          hasSnapshot = true;
          reportModelError(applied.snapshot);
        }
        return applied.snapshot;
      });
    };
    const acceptSubscription = (value: unknown) => {
      const subscription = chatSubscriptionSchema.safeParse(value);
      if (!subscription.success) {
        accept(value, conversationId);
        hasSnapshot = true;
        return;
      }
      if (subscription.data.protocolVersion > IPC_PROTOCOL_VERSION) {
        throw new Error("A conversa usa uma versão de protocolo mais recente.");
      }
      if (subscription.data.snapshot !== null) accept(subscription.data.snapshot, conversationId);
      subscription.data.batches.forEach(applyBatch);
      earlyBatches.splice(0).sort((left, right) => left.revision - right.revision).forEach(applyBatch);
      hasSnapshot = getChatSnapshot(conversationId) !== null;
    };
    const refresh = async () => {
      if (!active) return;
      if (refreshing) { refreshAgain = true; return; }
      refreshing = true;
      try {
        const cached = getChatSnapshot(conversationId);
        // Replaying an already-current cursor cannot repair an invalid page.
        const cursor = cached && hasValidHistoryWindow(cached) ? cached.revision : undefined;
        const value = await invoke<unknown>("subscribe_chat", { conversationId, ...(cursor === undefined ? {} : { cursor }) });
        if (active) acceptSubscription(value);
      } catch (cause) {
        if (active && !hasSnapshot) setError({ id: conversationId, message: libraryError(cause, "Não foi possível sincronizar esta conversa.") });
      } finally {
        refreshing = false;
        if (refreshAgain && active) { refreshAgain = false; void refresh(); }
      }
    };
    const scheduleResync = () => {
      if (resyncTimer || !active) return;
      resyncTimer = setTimeout(() => {
        resyncTimer = undefined;
        if (active) void refresh();
      }, 100);
    };
    // Subscribe before loading so an update cannot fall between snapshot and listener.
    const events = listen<unknown>("agent:event", event => {
      if (!active) return;
      const eventConversation = eventConversationId(event.payload);
      if (eventConversation !== conversationId) return;
      const parsed = agentEventBatchSchema.safeParse(event.payload);
      if (!parsed.success) { scheduleResync(); return; }
      const started = parsed.data.events.find(item => item.type === "turnStarted");
      if (started?.type === "turnStarted") {
        setPendingTurn(current => current?.conversationId === conversationId && matchesPendingTurn(started.turn, current.turn) ? null : current);
      }
      if (!getChatSnapshot(conversationId)) {
        earlyBatches.push(parsed.data);
        scheduleResync();
      } else applyBatch(parsed.data);
    });
    const workflowEvents = listen<unknown>("workflow:changed", event => {
      if (active && eventConversationId(event.payload) === conversationId) scheduleResync();
    });
    // Durable library invalidations are a coarse fallback for a dropped or
    // throttled streaming event. They are emitted at turn start and finish,
    // independently of the selected provider protocol.
    const libraryEvents = listen<unknown>("library:changed", event => {
      if (!active) return;
      const changedConversation = eventConversationId(event.payload);
      if (changedConversation === null || changedConversation === conversationId) scheduleResync();
    });
    void Promise.all([events, workflowEvents, libraryEvents]).then(unlisteners => {
      if (!active) { unlisteners.forEach(unlisten => unlisten()); return; }
      dispose.push(...unlisteners);
      stopResume = onDesktopResume(() => { void refresh(); });
      return refresh();
    }).catch((cause: unknown) => {
      if (active) setError({ id: conversationId, message: libraryError(cause, "Não foi possível abrir o histórico desta conversa.") });
    });
    return () => { active = false; if (resyncTimer) clearTimeout(resyncTimer); dispose.forEach(unlisten => unlisten()); stopResume?.(); if (generation.current === request) generation.current += 1; };
  }, [conversationId, attempt, accept, reportModelError]);

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
      updateChatSnapshot(id, current => current?.conversationId === id ? mergeHistory(current, page, direction) : current);
      return true;
    } catch (cause) {
      if (generation.current === request) setHistoryError({ id, message: libraryError(cause, "Não foi possível carregar este trecho.") });
      return false;
    } finally { if (sequence === historyRequest.current) { historyLock.current = null; setHistoryPending(null); } }
  };

  const send = async (content: string, options: TurnOptions, parts?: MessagePart[]): Promise<boolean> => {
    if (!conversationId || !snapshot || sending.current || compactLocks.current.has(conversationId) || snapshot.context?.compacting) return false;
    const id = conversationId; const request = generation.current;
    const optimisticId = `optimistic:${id}:${Date.now()}`;
    if (!snapshot.activeTurnId) {
      setPendingTurn({
        conversationId: id,
        turn: { id: optimisticId, createdAt: Date.now(), durationMs: 0, user: content, options, ...(parts?.length ? { parts } : {}), status: "running", tasks: [], steps: [], error: null },
      });
    }
    sending.current = true; setPendingId(id);
    try {
      const result = await invoke<unknown>("start_agent_turn", { conversationId: id, content, options, ...(parts?.length ? { parts } : {}) });
      if (generation.current === request) accept(result, id);
      return true;
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível enviar a mensagem. Seu texto foi mantido."));
      return false;
    } finally {
      sending.current = false; setPendingId(null);
      setPendingTurn(current => current?.turn.id === optimisticId ? null : current);
    }
  };
  const stop = async () => {
    if (!snapshot?.activeTurnId) return;
    try { await invoke("cancel_agent_turn", { conversationId, turnId: snapshot.activeTurnId }); }
    catch (cause) { toast.error(libraryError(cause, "Não foi possível interromper a execução.")); }
  };
  const approve = async (decision: ApprovalDecision): Promise<boolean> => {
    if (!snapshot?.activeTurnId || !snapshot.pendingApproval) return false;
    try {
      await invoke("approve_agent_tool", { conversationId, turnId: snapshot.activeTurnId, toolId: snapshot.pendingApproval.tool.id, decision });
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
  const deleteQueued = async (messageId: string): Promise<boolean> => {
    if (!conversationId) return false;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<unknown>("delete_queued_message", { conversationId: id, messageId });
      if (generation.current === request) accept(result, id);
      toast.success("Mensagem removida da fila");
      return true;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível excluir a mensagem agendada.")); return false; }
  };
  const reorderQueued = async (messageIds: string[]): Promise<boolean> => {
    if (!conversationId) return false;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<unknown>("reorder_queued_messages", { conversationId: id, messageIds });
      if (generation.current === request) accept(result, id);
      return true;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível reordenar as mensagens.")); return false; }
  };
  const sendQueuedNow = async (messageId: string): Promise<boolean> => {
    if (!conversationId) return false;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<{ delivered: boolean; snapshot: unknown }>("send_queued_message_now", { conversationId: id, messageId });
      if (generation.current === request) accept(result.snapshot, id);
      if (result.delivered) toast.success("Mensagem adicionada à execução atual");
      else toast.info("A execução já estava finalizando. A mensagem seguirá normalmente na fila.");
      return result.delivered;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível enviar a orientação agora.")); return false; }
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
  const answerAuthoring = async (proposal: PendingAuthoring, approved: boolean, note: string | null): Promise<boolean> => {
    if (!conversationId || snapshot?.activeTurnId !== proposal.turnId || snapshot.pendingAuthoring?.toolId !== proposal.toolId) return false;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<unknown>("answer_agent_authoring", { conversationId: id, decision: { turnId: proposal.turnId, toolId: proposal.toolId, approved, note } });
      if (generation.current === request) accept(result, id);
      if (approved && proposal.target.kind === "publication" && note?.trim()) {
        toast.info("Orientação enviada para revisão", { description: "O agente GitHub apresentará uma nova proposta antes de publicar." });
      } else {
        toast.success(approved ? proposal.target.kind === "publication" ? "Publicação processada" : "Configuração aprovada e salva" : "Proposta recusada");
      }
      return true;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível responder à proposta.")); return false; }
  };
  const resumeQueue = async () => {
    if (!conversationId) return;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<unknown>("resume_agent_queue", { conversationId: id });
      if (generation.current === request) accept(result, id);
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível continuar a fila.")); }
  };
  const resumeWorkflow = async (): Promise<boolean> => {
    if (!conversationId) return false;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<unknown>("resume_interrupted_workflow", { conversationId: id });
      if (generation.current === request) accept(result, id);
      toast.success("Fluxo retomado", { description: "O estado salvo será verificado antes de novas alterações." });
      return true;
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível retomar o fluxo interrompido."));
      return false;
    }
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
  return { snapshot, pendingTurn: pendingTurn?.conversationId === conversationId ? pendingTurn.turn : null, loadHistory, historyLoading: historyPending === conversationId && conversationId !== null, historyError: historyError?.id === conversationId ? historyError.message : null, error: error?.id === conversationId ? error.message : null, pending: pendingId === conversationId && conversationId !== null, compacting: (conversationId !== null && compactingIds.has(conversationId)) || snapshot?.context?.compacting === true, send, stop, approve, answerQuestion, answerAuthoring, removeQueued, deleteQueued, reorderQueued, sendQueuedNow, resumeQueue, resumeWorkflow, compact, retry: () => setAttempt(value => value + 1) };
}
export type ChatController = ReturnType<typeof useChat>;
