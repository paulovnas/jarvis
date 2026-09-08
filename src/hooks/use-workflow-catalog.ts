import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { libraryError } from "@/core/library";
import { workflowCatalogSchema, type CatalogMutation, type WorkflowCatalog } from "@/core/workflow-catalog";

export function useWorkflowCatalog() {
  const [data, setData] = useState<WorkflowCatalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const version = useRef(0);
  const mounted = useRef(false);
  const flight = useRef(false);
  const refresh = useCallback(async () => {
    const request = ++version.current;
    try {
      const value = workflowCatalogSchema.parse(await invoke("get_workflow_catalog"));
      if (mounted.current && request === version.current) { setData(current => current && current.revision > value.revision ? current : value); setError(null); }
    } catch (cause) { if (mounted.current && request === version.current) setError(libraryError(cause, "Não foi possível carregar o Workflow.")); }
  }, []);
  useEffect(() => {
    mounted.current = true;
    let active = true;
    let dispose: (() => void) | undefined;
    void listen("workflow-catalog:changed", () => { if (active) void refresh(); }).then(stop => {
      if (!active) { stop(); return; }
      dispose = stop; void refresh();
    }).catch(() => { if (active) void refresh(); });
    return () => { mounted.current = false; active = false; version.current += 1; dispose?.(); };
  }, [refresh]);
  const mutate = async (mutation: CatalogMutation, revision: number) => {
    if (flight.current) return false;
    flight.current = true; setSaving(true); ++version.current;
    try {
      const value = workflowCatalogSchema.parse(await invoke("mutate_workflow_catalog", { revision, mutation }));
      ++version.current;
      if (mounted.current) { setData(current => current && current.revision > value.revision ? current : value); setError(null); }
      void refresh();
      return true;
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível salvar. Suas alterações continuam no editor.")); void refresh(); return false; }
    finally { flight.current = false; if (mounted.current) setSaving(false); }
  };
  return { data, error, saving, refresh, mutate };
}
export type WorkflowCatalogController = ReturnType<typeof useWorkflowCatalog>;
