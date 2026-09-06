import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { coreError, coreSnapshotSchema, type CoreId, type CoreSnapshot } from "@/core/core-components";

export function useCore() {
  const [snapshot, setSnapshot] = useState<CoreSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const mounted = useRef(false);
  const revision = useRef(0);
  const busy = useRef(false);
  const accept = useCallback((value: unknown) => {
    const parsed = coreSnapshotSchema.parse(value);
    if (mounted.current) { revision.current += 1; setSnapshot(parsed); setError(null); }
    return parsed;
  }, []);
  const refresh = useCallback(async () => {
    const version = ++revision.current;
    try { const value = await invoke("get_core_status"); if (version === revision.current) accept(value); }
    catch (cause) { if (mounted.current && version === revision.current) setError(coreError(cause)); }
  }, [accept]);
  useEffect(() => {
    mounted.current = true;
    let unlisten: (() => void) | undefined;
    void listen("core:changed", event => { if (mounted.current) { try { accept(event.payload); } catch { void refresh(); } } }).then(stop => { if (mounted.current) unlisten = stop; else stop(); }).catch(() => { /* Explicit commands remain available if event registration fails. */ });
    void refresh();
    return () => { mounted.current = false; revision.current += 1; unlisten?.(); };
  }, [accept, refresh]);
  const check = useCallback(async () => {
    try { accept(await invoke("check_core_updates")); }
    catch (cause) { if (mounted.current) toast.error(coreError(cause)); }
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
