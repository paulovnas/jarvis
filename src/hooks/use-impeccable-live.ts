import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import { toast } from "sonner";
import type { TurnOptions } from "@/core/chat";
import { libraryError } from "@/core/library";

const liveSchema = z.object({
  conversationId: z.string(),
  state: z.enum(["off", "starting", "ready", "setup"]),
  url: z.string().nullable(),
  tabId: z.string().nullable(),
  error: z.string().nullable(),
  setupNeeded: z.record(z.string(), z.unknown()).nullable(),
});
type LiveStatus = z.infer<typeof liveSchema>;
const off = (conversationId: string): LiveStatus => ({ conversationId, state: "off", url: null, tabId: null, error: null, setupNeeded: null });
function localPage(value?: string) {
  try {
    const url = new URL(value ?? "");
    return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password && (url.hostname === "localhost" || url.hostname === "[::1]" || /^127\.\d+\.\d+\.\d+$/.test(url.hostname)) ? url.href : undefined;
  } catch { return undefined; }
}

export function useImpeccableLive(conversationId: string, enabled = true) {
  const [status, setStatus] = useState(() => off(conversationId));
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const operation = useRef(false);
  const generation = useRef({ version: 0, conversationId, alive: true });
  useEffect(() => {
    const epoch = generation.current;
    epoch.conversationId = conversationId;
    epoch.alive = true;
    if (!enabled) return;
    let alive = true;
    const request = ++epoch.version;
    const subscription = listen<unknown>("impeccable-live:changed", event => {
      const next = liveSchema.safeParse(event.payload);
      if (!alive || !next.success || next.data.conversationId !== conversationId) return;
      ++epoch.version;
      setStatus(next.data);
      setLoaded(true);
      if (next.data.error) toast.error("Modo Live", { id: `impeccable-live:${conversationId}`, description: next.data.error });
    });
    void invoke<unknown>("get_impeccable_live", { conversationId }).then(value => {
      if (!alive || epoch.version !== request) return;
      const next = liveSchema.parse(value);
      if (next.conversationId !== conversationId) throw new Error("A sessão Live pertence a outra conversa.");
      setStatus(next); setLoaded(true);
    }).catch(cause => {
      if (!alive || epoch.version !== request) return;
      setLoaded(true);
      toast.error(libraryError(cause, "Não foi possível consultar o modo Live."));
    });
    return () => { alive = false; epoch.alive = false; ++epoch.version; void subscription.then(unlisten => unlisten()).catch(() => {}); };
  }, [conversationId, enabled]);
  const activeStatus = status.conversationId === conversationId ? status : off(conversationId);
  const active = activeStatus.state !== "off";
  async function toggle(options?: TurnOptions, url?: string) {
    if (!enabled || operation.current || (!active && !options)) return;
    operation.current = true; setBusy(true);
    const epoch = generation.current;
    const request = ++epoch.version;
    try {
      const page = localPage(url);
      const next = liveSchema.parse(await invoke<unknown>(active ? "stop_impeccable_live" : "start_impeccable_live", {
        conversationId, ...(active ? {} : { options, ...(page ? { url: page } : {}) }),
      }));
      if (epoch.alive && epoch.version === request && epoch.conversationId === conversationId && next.conversationId === conversationId) setStatus(next);
    } catch (cause) {
      toast.error(libraryError(cause, active ? "Não foi possível desligar o modo Live." : "Não foi possível iniciar o modo Live."));
    } finally { operation.current = false; setBusy(false); }
  }
  return { status: activeStatus, active, loaded: loaded && status.conversationId === conversationId, busy: busy || activeStatus.state === "starting", toggle };
}
export type ImpeccableLiveController = ReturnType<typeof useImpeccableLive>;
