import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { fileChangeSchema, type FileChange } from "@/core/chat";
import { libraryError } from "@/core/library";

export function useSessionFiles(conversationId: string | null) {
  const [result, setResult] = useState<{ id: string; files: FileChange[]; error?: string } | null>(null);
  useEffect(() => {
    if (!conversationId) return;
    let active = true;
    let running = false;
    let queued = false;
    let stop: (() => void) | undefined;
    let debounce: ReturnType<typeof setTimeout> | undefined;
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
    void listen<{ conversationId: string; activeTurnId: string | null; fileChanges?: FileChange[] }>("agent:updated", event => {
      if (!active || event.payload.conversationId !== conversationId) return;
      const next = JSON.stringify([event.payload.activeTurnId, event.payload.fileChanges]);
      if (next === fingerprint) return;
      fingerprint = next;
      // Streaming emits frequently. Throttle without postponing forever.
      if (!debounce) debounce = setTimeout(() => { debounce = undefined; void refresh(); }, event.payload.activeTurnId ? 1000 : 0);
    }).then(unlisten => { if (active) stop = unlisten; else unlisten(); }).catch(() => {});
    const focus = () => { if (document.visibilityState === "visible") void refresh(); };
    const timer = setInterval(focus, 10_000);
    window.addEventListener("focus", focus);
    document.addEventListener("visibilitychange", focus);
    return () => { active = false; stop?.(); clearInterval(timer); clearTimeout(debounce); window.removeEventListener("focus", focus); document.removeEventListener("visibilitychange", focus); };
  }, [conversationId]);
  const selected = result?.id === conversationId ? result : null;
  return { files: selected?.files ?? [], error: selected?.error, loading: !!conversationId && !selected };
}
