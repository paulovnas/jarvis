import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { browserError, browserExtensionStatusSchema, type BrowserExtensionStatus } from "@/core/browser";

export function useBrowserExtensionStatus(enabled = true) {
  const [status, setStatus] = useState<BrowserExtensionStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const updateRef = useRef<(() => Promise<void>) | null>(null);
  const refresh = useCallback(() => { void updateRef.current?.(); }, []);
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    let pending = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const update = async () => {
      if (!alive || pending) return;
      pending = true;
      clearTimeout(timer);
      try {
        const next = browserExtensionStatusSchema.parse(await invoke("get_browser_extension_status"));
        if (alive) { setStatus(next); setError(null); }
      } catch (cause) {
        if (alive) setError(browserError(cause));
      } finally {
        pending = false;
        if (alive) timer = setTimeout(() => void update(), 5000);
      }
    };
    const subscription = listen("browser-extension:changed", () => void update());
    void subscription.catch(cause => { if (alive) setError(browserError(cause)); });
    void update();
    updateRef.current = update;
    return () => { alive = false; updateRef.current = null; clearTimeout(timer); void subscription.then(stop => stop()).catch(() => {}); };
  }, [enabled]);
  return { status, error, refresh };
}
