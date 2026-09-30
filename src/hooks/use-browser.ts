import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { browserSnapshotSchema, browserTabSchema, EMPTY_BROWSER, type BrowserRequest } from "@/core/browser";
import { browserError as libraryError } from "@/core/browser";

export function useBrowser(conversationId: string, onActivate?: () => void, enabled = true) {
  const [snapshot, setSnapshot] = useState(EMPTY_BROWSER);
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const generation = useRef({ version: 0, selection: 0, conversationId, alive: true });
  const active = useRef<string | null>(null);
  const activate = useRef(onActivate);
  useEffect(() => { activate.current = onActivate; }, [onActivate]);
  const refresh = useCallback(async () => {
    const version = ++generation.current.version;
    const result = browserSnapshotSchema.parse(await invoke("get_browser_tabs", { conversationId }));
    if (!generation.current.alive || conversationId !== generation.current.conversationId || version !== generation.current.version) return;
    // The extension's active target belongs to the agent, not the visible workspace.
    const external = result.backend === "extension" || result.activeId?.startsWith("ext:");
    const activeId = external ? result.tabs.find(tab => tab.id === active.current)?.id ?? null : result.activeId;
    if (activeId && activeId !== active.current) activate.current?.();
    active.current = activeId;
    setSnapshot({ ...result, activeId });
    setLoaded(true);
  }, [conversationId]);
  useEffect(() => {
    if (!enabled) return;
    generation.current.conversationId = conversationId;
    generation.current.alive = true;
    let alive = true;
    const token = generation.current;
    const update = () => { if (alive) void refresh().catch(cause => { if (alive) toast.error(libraryError(cause), { id: `browser-load:${conversationId}` }); }); };
    const subscription = listen<{ conversationId: string }>("browser:changed", event => { if (event.payload.conversationId === conversationId) update(); });
    const settingsSubscription = listen("system:changed", update);
    const connectionSubscription = listen("browser-extension:changed", update);
    update();
    return () => { alive = false; token.alive = false; token.version++; for (const listener of [subscription, settingsSubscription, connectionSubscription]) void listener.then(unlisten => unlisten()).catch(() => {}); void invoke("set_browser_viewport", { conversationId, id: null, viewport: null }).catch(() => {}); };
  }, [conversationId, refresh, enabled]);
  const command = useCallback(async (request: BrowserRequest) => {
    const selection = request.action === "open" || request.action === "attach" ? ++generation.current.selection : generation.current.selection;
    try {
      const result: unknown = await invoke("browser_command", { conversationId, request });
      if (!generation.current.alive || generation.current.conversationId !== conversationId) return result;
      if (request.action === "open" || request.action === "attach") {
        const opened = browserSnapshotSchema.safeParse(result);
        const tab = browserTabSchema.safeParse(opened.success ? opened.data.tabs.find(item => item.id === opened.data.activeId) : result);
        if (tab.success && tab.data.conversationId === conversationId && generation.current.selection === selection) {
          active.current = tab.data.id;
          activate.current?.();
        }
      }
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
    generation.current.selection++;
    if (id) activate.current?.();
    active.current = id;
    setSnapshot(current => ({ ...current, activeId: id }));
    void command({ action: "select", id });
  }, [command]);
  return { conversationId, snapshot, busy, loaded, open, select, command };
}
export type BrowserController = ReturnType<typeof useBrowser>;
