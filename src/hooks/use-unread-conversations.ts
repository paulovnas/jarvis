import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";
import { z } from "zod";

const snapshotSchema = z.object({
  revision: z.number().int().nonnegative(),
  conversations: z.array(z.object({ conversationId: z.string(), eventKey: z.string() })),
});
type Snapshot = z.infer<typeof snapshotSchema>;

/** Read only the visible latest transcript, never every session on app focus. */
export function useUnreadConversations(visibleConversationId: string | null) {
  const [snapshot, setSnapshot] = useState<Snapshot>({ revision: 0, conversations: [] });
  const [focus, setFocus] = useState({ focused: document.hasFocus(), epoch: 0 });
  const revision = useRef(-1);
  const accept = useCallback((value: unknown) => {
    const next = snapshotSchema.parse(value);
    if (next.revision <= revision.current) return;
    revision.current = next.revision;
    setSnapshot(next);
  }, []);
  useEffect(() => {
    let alive = true;
    const stops: (() => void)[] = [];
    const updateFocus = (focused: boolean) => { if (alive) setFocus(current => ({ focused, epoch: current.epoch + 1 })); };
    const refresh = async () => {
      const value = await invoke<unknown>("get_unread_conversations");
      if (alive) accept(value);
    };
    const onFocus = () => {
      updateFocus(true);
      // Recover events that the WebView may have missed while suspended.
      void refresh().catch(() => {});
    };
    const onBlur = () => updateFocus(false);
    const onVisibility = () => { if (document.visibilityState === "visible" && document.hasFocus()) onFocus(); else onBlur(); };
    const onFocusIn = () => { if (document.hasFocus()) updateFocus(true); };
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    document.addEventListener("visibilitychange", onVisibility);
    document.addEventListener("focusin", onFocusIn);
    const subscribe = async (event: string, handler: (value: unknown) => void) => {
      const stop = await listen<unknown>(event, message => { if (alive) handler(message.payload); });
      if (alive) stops.push(stop); else stop();
    };
    const subscribeNativeFocus = async () => {
      const appWindow = getCurrentWindow();
      const stop = await appWindow.onFocusChanged(({ payload }) => { if (payload) onFocus(); else onBlur(); });
      if (alive) stops.push(stop); else stop();
      const focused = await appWindow.isFocused();
      if (alive) updateFocus(focused);
    };
    void Promise.all([
      subscribe("unread:changed", value => { try { accept(value); } catch { /* Ignore malformed/stale broadcasts. */ } }),
      subscribe("unread:error", () => toast.error("Não foi possível salvar o estado de leitura.", { id: "unread-sync" })),
      subscribeNativeFocus().catch(() => { /* DOM and Tauri focus events remain as fallbacks. */ }),
      subscribe("tauri://focus", onFocus),
      subscribe("tauri://blur", onBlur),
    ]).then(async () => {
      if (!alive) return;
      await refresh();
    }).catch(() => {
      if (alive) toast.error("Não foi possível sincronizar as mensagens não lidas.", { id: "unread-sync" });
    });
    return () => {
      alive = false; stops.forEach(stop => stop());
      window.removeEventListener("focus", onFocus); window.removeEventListener("blur", onBlur);
      document.removeEventListener("visibilitychange", onVisibility); document.removeEventListener("focusin", onFocusIn);
    };
  }, [accept]);

  const eventKey = snapshot.conversations.find(item => item.conversationId === visibleConversationId)?.eventKey;
  useEffect(() => {
    if (!visibleConversationId || !eventKey || !focus.focused || document.visibilityState !== "visible") return;
    let alive = true;
    // Let the latest message paint; brief navigation/focus changes are not reads.
    const timer = setTimeout(() => {
      if (document.querySelector('[role="dialog"], [role="alertdialog"]')) return;
      void invoke<unknown>("mark_conversation_read", { conversationId: visibleConversationId, eventKey })
        .then(value => { if (alive) accept(value); })
        .catch(() => { if (alive) toast.error("Não foi possível marcar a conversa como lida.", { id: "unread-sync" }); });
    }, 350);
    return () => { alive = false; clearTimeout(timer); };
  }, [visibleConversationId, eventKey, focus, accept]);

  return new Set(snapshot.conversations.map(item => item.conversationId));
}
