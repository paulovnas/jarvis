import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { systemSnapshotSchema } from "@/core/system-preferences";

/** Keep failed OS deliveries visible even when settings are closed. */
export function useNotificationFeedback() {
  useEffect(() => {
    let alive = true;
    let previous: string | null = null;
    let unlisten: (() => void) | undefined;
    void listen("system:changed", event => {
      if (!alive) return;
      const parsed = systemSnapshotSchema.safeParse(event.payload);
      if (!parsed.success) return;
      const error = parsed.data.preferences.notifications ? parsed.data.notificationError : null;
      if (error && error !== previous) {
        toast.error("Não foi possível enviar a notificação do sistema", {
          id: "system-notification-error", description: error, duration: 12_000,
        });
      }
      previous = error;
    }).then(stop => { if (alive) unlisten = stop; else stop(); }).catch(() => {
      // The web preview has no desktop notification service.
    });
    return () => { alive = false; unlisten?.(); };
  }, []);
}
