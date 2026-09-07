import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { agentActivitySchema } from "@/core/chat";
import { onDesktopResume } from "@/core/desktop-resume";

// Observe every session independently of the conversation currently open in the chat.
export function useAgentActivity() {
  const [runningIds, setRunningIds] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    let active = true;
    let dispose: (() => void) | undefined;
    let stopResume: (() => void) | undefined;
    let refreshing = false;
    let refreshAgain = false;
    const latest = new Map<string, { revision: number; activeTurnId: string | null }>();
    const accept = (value: unknown) => {
      const next = agentActivitySchema.safeParse(value);
      if (!next.success) return;
      const { conversationId, revision, activeTurnId, compacting } = next.data;
      const previous = latest.get(conversationId);
      if (previous && previous.revision >= revision) return;
      latest.set(conversationId, { revision, activeTurnId });
      setRunningIds(current => {
        const running = activeTurnId !== null || compacting === true;
        if (current.has(conversationId) === running) return current;
        const updated = new Set(current);
        if (running) updated.add(conversationId);
        else updated.delete(conversationId);
        return updated;
      });
    };
    const refresh = async () => {
      if (!active) return;
      if (refreshing) { refreshAgain = true; return; }
      refreshing = true;
      const before = new Map(latest);
      try {
        const value = agentActivitySchema.array().parse(await invoke<unknown>("get_agent_activity"));
        if (!active) return;
        const present = new Set(value.map(item => item.conversationId));
        const finished = new Set<string>();
        // Idle sessions may already have been evicted. Preserve any event that
        // arrived after this request started, including a newly running turn.
        for (const [id, previous] of before) {
          if (!present.has(id) && latest.get(id) === previous) {
            latest.set(id, { ...previous, activeTurnId: null }); finished.add(id);
          }
        }
        if (finished.size) setRunningIds(current => new Set([...current].filter(id => !finished.has(id))));
        value.forEach(accept);
      } catch {
        if (active) toast.error("Não foi possível sincronizar os indicadores de execução.", { id: "agent-activity-sync" });
      } finally {
        refreshing = false;
        if (active && refreshAgain) { refreshAgain = false; void refresh(); }
      }
    };
    void listen<unknown>("agent:updated", event => { if (active) accept(event.payload); }).then(unlisten => {
      if (!active) { unlisten(); return; }
      dispose = unlisten;
      stopResume = onDesktopResume(() => { void refresh(); });
      return refresh();
    }).catch(() => {
      if (active) toast.error("Não foi possível sincronizar os indicadores de execução. Reabra o aplicativo para tentar novamente.");
    });
    return () => { active = false; dispose?.(); stopResume?.(); };
  }, []);
  return runningIds;
}
