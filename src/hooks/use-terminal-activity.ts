import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { terminalConversationActivitySchema } from "@/core/terminals";
import { onDesktopResume } from "@/core/desktop-resume";

/** Observe terminal tabs across every conversation without loading each transcript. */
export function useTerminalActivity() {
  const [counts, setCounts] = useState<ReadonlyMap<string, number>>(() => new Map());

  useEffect(() => {
    let active = true;
    let dispose: (() => void) | undefined;
    let stopResume: (() => void) | undefined;
    let refreshing = false;
    let refreshAgain = false;

    const refresh = async () => {
      if (!active) return;
      if (refreshing) {
        refreshAgain = true;
        return;
      }
      refreshing = true;
      try {
        const snapshot = terminalConversationActivitySchema.array().parse(
          await invoke<unknown>("get_terminal_activity"),
        );
        if (active) setCounts(new Map(snapshot.map(item => [item.conversationId, item.count])));
      } catch {
        if (active) toast.error("Não foi possível sincronizar os indicadores de terminais.", { id: "terminal-activity-sync" });
      } finally {
        refreshing = false;
        if (active && refreshAgain) {
          refreshAgain = false;
          void refresh();
        }
      }
    };

    void listen("terminals:changed", () => { void refresh(); }).then(unlisten => {
      if (!active) {
        unlisten();
        return;
      }
      dispose = unlisten;
      stopResume = onDesktopResume(() => { void refresh(); });
      return refresh();
    }).catch(() => {
      if (active) toast.error("Não foi possível acompanhar os terminais. Reabra o aplicativo para tentar novamente.", { id: "terminal-activity-sync" });
    });

    return () => {
      active = false;
      dispose?.();
      stopResume?.();
    };
  }, []);

  return counts;
}
