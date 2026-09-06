import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { coreDownloadEventSchema, coreError, coreSnapshotSchema, type CoreId, type CoreSnapshot } from "@/core/core-components";

export function useCore() {
  const [snapshot, setSnapshot] = useState<CoreSnapshot | null>(null);
  const currentSnapshot = useRef<CoreSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const mounted = useRef(false);
  const revision = useRef(0);
  const busy = useRef(false);
  const accept = useCallback((value: unknown) => {
    const parsed = coreSnapshotSchema.parse(value);
    if (mounted.current) { revision.current += 1; currentSnapshot.current = parsed; setSnapshot(parsed); setError(null); }
    return parsed;
  }, []);
  const refresh = useCallback(async () => {
    const version = ++revision.current;
    try { const value = await invoke("get_core_status"); if (version === revision.current) accept(value); }
    catch (cause) { if (mounted.current && version === revision.current) setError(coreError(cause)); }
  }, [accept]);
  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    const stops: (() => void)[] = [];
    const keep = (stop: () => void) => { if (disposed) stop(); else stops.push(stop); };
    void Promise.allSettled([
      listen("core:changed", event => { if (!disposed) { try { accept(event.payload); } catch { void refresh(); } } }).then(keep),
      listen("core:download", event => {
        const parsed = coreDownloadEventSchema.safeParse(event.payload);
        const current = currentSnapshot.current;
        if (disposed || !parsed.success || !current || !current.items.some(item => item.id === parsed.data.id && item.stage !== null)) return;
        revision.current += 1;
        const next = { ...current, items: current.items.map(item => item.id === parsed.data.id ? { ...item, download: parsed.data.download } : item) };
        currentSnapshot.current = next;
        setSnapshot(next);
      }).then(keep),
    ]).then(() => { if (!disposed) void refresh(); });
    return () => { disposed = true; mounted.current = false; revision.current += 1; stops.forEach(stop => stop()); };
  }, [accept, refresh]);
  const check = useCallback(async () => {
    const version = ++revision.current;
    try {
      const value = await invoke("check_core_updates");
      if (version === revision.current) accept(value);
    }
    catch (cause) { if (mounted.current && version === revision.current) toast.error(coreError(cause)); }
  }, [accept]);
  const install = useCallback(async (ids: CoreId[]) => {
    if (busy.current) return;
    busy.current = true; setInstalling(true);
    try {
      for (const id of ids) accept(await invoke("install_core_component", { id }));
      toast.success(ids.length > 1 ? "Core instalado" : "Componente pronto");
    } catch (cause) { toast.error(coreError(cause)); await refresh(); }
    finally { busy.current = false; if (mounted.current) setInstalling(false); }
  }, [accept, refresh]);
  return { snapshot, error, refresh, check, install, busy: installing || snapshot?.items.some(item => item.stage !== null) === true };
}
export type CoreController = ReturnType<typeof useCore>;
