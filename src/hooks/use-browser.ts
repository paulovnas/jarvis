import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { browserSnapshotSchema, EMPTY_BROWSER, type BrowserRequest } from "@/core/browser";
import { browserError as libraryError } from "@/core/browser";

export function useBrowser(conversationId: string, onActivate?: () => void, enabled = true) {
  const [snapshot, setSnapshot] = useState(EMPTY_BROWSER);
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const generation = useRef({ version: 0 });
  const active = useRef<string | null>(null);
  const activate = useRef(onActivate);
  useEffect(() => { activate.current = onActivate; }, [onActivate]);
  const refresh = useCallback(async () => {
    const version = ++generation.current.version;
    const result = browserSnapshotSchema.parse(await invoke("get_browser_tabs", { conversationId }));
    if (version !== generation.current.version) return;
    if (result.activeId && result.activeId !== active.current) activate.current?.();
    active.current = result.activeId;
    setSnapshot(result);
    setLoaded(true);
  }, [conversationId]);
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    const token = generation.current;
    const update = () => { if (alive) void refresh().catch(cause => { if (alive) toast.error(libraryError(cause), { id: `browser-load:${conversationId}` }); }); };
    const subscription = listen<{ conversationId: string }>("browser:changed", event => { if (event.payload.conversationId === conversationId) update(); });
    update();
    return () => { alive = false; token.version++; void subscription.then(unlisten => unlisten()); void invoke("set_browser_viewport", { conversationId, id: null, viewport: null }).catch(() => {}); };
  }, [conversationId, refresh, enabled]);
  const command = useCallback(async (request: BrowserRequest) => {
    try {
      const result: unknown = await invoke("browser_command", { conversationId, request });
      await refresh();
      return result;
    } catch (cause) { toast.error(libraryError(cause)); return undefined; }
  }, [conversationId, refresh]);
  const open = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    try { await command({ action: "open" }); } finally { setBusy(false); }
  }, [busy, command]);
  const select = useCallback((id: string | null) => {
    if (id) activate.current?.();
    active.current = id;
    setSnapshot(current => ({ ...current, activeId: id }));
    void command({ action: "select", id });
  }, [command]);
  return { conversationId, snapshot, busy, loaded, open, select, command };
}
export type BrowserController = ReturnType<typeof useBrowser>;
