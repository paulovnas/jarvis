import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { queuedMessageSchema, readChat, type ChatSnapshot, type TurnOptions } from "@/core/chat";
import { libraryError } from "@/core/library";

export function useChat(conversationId: string | null) {
  const [loaded, setLoaded] = useState<ChatSnapshot | null>(null);
  const [error, setError] = useState<{ id: string; message: string } | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const generation = useRef(0);
  const sending = useRef(false);
  const compactLocks = useRef(new Set<string>());
  const [compactingIds, setCompactingIds] = useState<ReadonlySet<string>>(() => new Set());
  const snapshot = loaded?.conversationId === conversationId ? loaded : null;

  const accept = useCallback((value: unknown, id: string) => {
    const next = readChat(value, id);
    setLoaded(current => current?.conversationId === id && current.revision > next.revision ? current : next);
  }, []);

  useEffect(() => {
    const request = ++generation.current;
    if (!conversationId) return;
    let dispose: (() => void) | undefined;
    let active = true;
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
      return invoke<unknown>("get_chat", { conversationId }).then(value => {
        if (active) { accept(value, conversationId); setError(null); }
      });
    }).catch((cause: unknown) => {
      if (active) setError({ id: conversationId, message: libraryError(cause, "Não foi possível abrir o histórico desta conversa.") });
    });
    return () => { active = false; dispose?.(); if (generation.current === request) generation.current += 1; };
  }, [conversationId, attempt, accept]);

  const send = async (content: string, options: TurnOptions): Promise<boolean> => {
    if (!conversationId || !snapshot || sending.current || compactLocks.current.has(conversationId) || snapshot.context?.compacting) return false;
    const id = conversationId; const request = generation.current;
    sending.current = true; setPendingId(id);
    try {
      const result = await invoke<unknown>("start_agent_turn", { conversationId: id, content, options });
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
  const removeQueued = async (messageId: string): Promise<string | null> => {
    if (!conversationId) return null;
    const id = conversationId; const request = generation.current;
    try {
      const result = await invoke<{ message: unknown; snapshot: unknown }>("remove_queued_message", { conversationId: id, messageId });
      const message = queuedMessageSchema.parse(result.message);
      const next = readChat(result.snapshot, id);
      if (generation.current === request) accept(next, id);
      return message.content;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível retirar a mensagem da fila.")); return null; }
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
  return { snapshot, error: error?.id === conversationId ? error.message : null, pending: pendingId === conversationId && conversationId !== null, compacting: (conversationId !== null && compactingIds.has(conversationId)) || snapshot?.context?.compacting === true, send, stop, approve, removeQueued, resumeQueue, compact, retry: () => setAttempt(value => value + 1) };
}
export type ChatController = ReturnType<typeof useChat>;
