import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import { toast } from "sonner";

const sessionSchema = z.object({ conversationId: z.string(), sessionId: z.string() });
export function requestPixelArtSuite(conversationId: string) {
  window.dispatchEvent(new CustomEvent("pixel-art:open", { detail: { conversationId } }));
}

export function usePixelArt(conversationId: string, enabled = true) {
  const [available, setAvailable] = useState(false);
  const [active, setActive] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(false);
  const opening = useRef(false);
  const open = useCallback(async () => {
    if (opening.current) return;
    opening.current = true;
    setBusy(true); setError(null);
    try {
      const result = sessionSchema.parse(await invoke("open_pixel_art_suite", { conversationId }));
      if (alive.current && result.conversationId === conversationId) { setAvailable(true); setActive(true); }
    } catch (cause) {
      if (alive.current) {
        const message = cause instanceof Error ? cause.message : typeof cause === "string" ? cause : "Não foi possível abrir a suíte de pixel art.";
        setError(message); toast.error(message);
      }
    } finally { opening.current = false; if (alive.current) setBusy(false); }
  }, [conversationId]);
  useEffect(() => {
    if (!enabled) return;
    alive.current = true;
    let current = true;
    const accept = (payload: unknown) => {
      const parsed = sessionSchema.safeParse(payload);
      if (current && parsed.success && parsed.data.conversationId === conversationId) setAvailable(true);
    };
    const subscription = listen<unknown>("pixel-art-session", event => accept(event.payload));
    void subscription.then(async () => {
      if (current) accept(await invoke("get_pixel_art_session", { conversationId }));
    }).catch(() => undefined);
    const requested = (event: Event) => {
      if (event instanceof CustomEvent && event.detail?.conversationId === conversationId) void open();
    };
    window.addEventListener("pixel-art:open", requested);
    return () => {
      current = false; alive.current = false;
      window.removeEventListener("pixel-art:open", requested);
      void subscription.then(unlisten => unlisten()).catch(() => undefined);
      void invoke("set_pixel_art_viewport", { conversationId, viewport: null }).catch(() => undefined);
    };
  }, [conversationId, enabled, open]);
  const close = useCallback(() => { setActive(false); setAvailable(false); }, []);
  return { conversationId, available, active, busy, error, select: setActive, open, close };
}
export type PixelArtController = ReturnType<typeof usePixelArt>;
