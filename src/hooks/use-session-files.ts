import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { fileChangeSchema, type FileChange } from "@/core/chat";
import { agentEventBatchSchema } from "@/core/agent-events";
import { libraryError } from "@/core/library";

export function useSessionFiles(conversationId: string | null) {
  const [result, setResult] = useState<{ id: string; files: FileChange[]; error?: string } | null>(null);
  useEffect(() => {
    if (!conversationId) return;
    let active = true;
    let running = false;
    let queued = false;
    let stop: (() => void) | undefined;
    let fingerprint: string | undefined;
    const refresh = async () => {
      if (!active) return;
      if (running) { queued = true; return; }
      running = true;
      try {
        const files = fileChangeSchema.array().parse(await invoke("get_agent_file_changes", { conversationId }));
        if (active) setResult({ id: conversationId, files });
      } catch (cause) {
        // Do not present a previously valid list as confirmed after a failed refresh.
        if (active) setResult({ id: conversationId, files: [], error: libraryError(cause, "Não foi possível conferir as alterações.") });
      } finally {
        running = false;
        if (active && queued) { queued = false; void refresh(); }
      }
    };
    void refresh();
    void listen<unknown>("agent:event", event => {
      if (!active) return;
      const parsed = agentEventBatchSchema.safeParse(event.payload);
      if (!parsed.success || parsed.data.conversationId !== conversationId) return;
      const changed = [...parsed.data.events].reverse().find(item => item.type === "stateChanged");
      if (!changed || changed.type !== "stateChanged") return;
      const next = JSON.stringify([changed.state.activeTurnId, changed.state.fileChanges]);
      if (next === fingerprint) return;
      fingerprint = next;
      setResult({ id: conversationId, files: changed.state.fileChanges });
    }).then(unlisten => { if (active) stop = unlisten; else unlisten(); }).catch(() => {});
    const focus = () => { if (document.visibilityState === "visible") void refresh(); };
    const timer = setInterval(focus, 10_000);
    window.addEventListener("focus", focus);
    document.addEventListener("visibilitychange", focus);
    return () => { active = false; stop?.(); clearInterval(timer); window.removeEventListener("focus", focus); document.removeEventListener("visibilitychange", focus); };
  }, [conversationId]);
  const selected = result?.id === conversationId ? result : null;
  return { files: selected?.files ?? [], error: selected?.error, loading: !!conversationId && !selected };
}
