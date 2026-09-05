import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { agentActivitySchema } from "@/core/chat";

// Observe every session independently of the conversation currently open in the chat.
export function useAgentActivity() {
  const [runningIds, setRunningIds] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    let active = true;
    let dispose: (() => void) | undefined;
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
    void listen<unknown>("agent:updated", event => { if (active) accept(event.payload); }).then(unlisten => {
      if (!active) { unlisten(); return; }
      dispose = unlisten;
      return invoke<unknown>("get_agent_activity").then(value => {
        if (!active) return;
        for (const item of agentActivitySchema.array().parse(value)) accept(item);
      });
    }).catch(() => {
      if (active) toast.error("Não foi possível sincronizar os indicadores de execução. Reabra o aplicativo para tentar novamente.");
    });
    return () => { active = false; dispose?.(); };
  }, []);
  return runningIds;
}
