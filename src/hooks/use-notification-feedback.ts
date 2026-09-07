import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { systemSnapshotSchema } from "@/core/system-preferences";
import { onDesktopResume } from "@/core/desktop-resume";

/** Keep failed OS deliveries visible even when settings are closed. */
export function useNotificationFeedback() {
  useEffect(() => {
    let alive = true;
    let previous: string | null = null;
    let pending: string | null = null;
    let version = 0;
    let unlisten: (() => void) | undefined;
    const present = () => {
      if (!alive || document.visibilityState !== "visible" || !document.hasFocus()) return;
      if (pending && pending !== previous) {
        toast.error("Não foi possível enviar a notificação do sistema", {
          id: "system-notification-error", description: pending, duration: 12_000,
        });
      }
      previous = pending;
    };
    const accept = (value: unknown) => {
      if (!alive) return;
      const parsed = systemSnapshotSchema.safeParse(value);
      if (!parsed.success) return;
      pending = parsed.data.preferences.notifications ? parsed.data.notificationError : null;
      if (!pending) previous = null;
      present();
    };
    const refresh = async () => {
      present();
      const request = ++version;
      try {
        const value = await invoke<unknown>("get_system_preferences");
        if (alive && request === version) accept(value);
      } catch { /* Browser previews have no notification service. */ }
    };
    const stopResume = onDesktopResume(() => { void refresh(); });
    void listen("system:changed", event => {
      version += 1; accept(event.payload);
    }).then(stop => { if (alive) { unlisten = stop; void refresh(); } else stop(); }).catch(() => {
      // The web preview has no desktop notification service.
    });
    return () => { alive = false; unlisten?.(); stopResume(); };
  }, []);
}
