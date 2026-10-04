import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";

const statusSchema = z.object({
  projectId: z.string(),
  eligible: z.boolean(),
  enabled: z.boolean(),
  reason: z.string().nullish(),
});
const sourceSchema = z.object({
  id: z.string(),
  projectId: z.string(),
  projectName: z.string(),
  title: z.string(),
  status: z.string(),
});
const incidentSchema = z.object({
  id: z.string(),
  capturedAt: z.number().int().nonnegative(),
  conversationTitle: z.string(),
  sourceProjectName: z.string(),
  sourceStatus: z.string(),
  eventCount: z.number().int().nonnegative(),
  truncated: z.boolean(),
  reference: z.string(),
});

export type SelfDevelopmentStatus = z.infer<typeof statusSchema>;
export type SelfDevelopmentSource = z.infer<typeof sourceSchema>;
export type SelfDevelopmentIncident = z.infer<typeof incidentSchema>;

type Result = {
  projectId: string;
  status: SelfDevelopmentStatus | null;
  sources: SelfDevelopmentSource[];
  incidents: SelfDevelopmentIncident[];
  loading: boolean;
  pending: boolean;
  error: string | null;
};

function empty(projectId: string): Result {
  return { projectId, status: null, sources: [], incidents: [], loading: true, pending: false, error: null };
}

function readStatus(value: unknown, projectId: string): SelfDevelopmentStatus {
  const status = statusSchema.parse(value);
  if (status.projectId !== projectId || (status.enabled && !status.eligible)) throw new Error("Invalid project authorization");
  return status;
}

/** Native identity and authorization are authoritative; stale requests cannot cross projects. */
export function useSelfDevelopment(projectId: string) {
  const [result, setResult] = useState<Result | null>(null);
  const generation = useRef(0);
  const busy = useRef(false);
  const refreshPending = useRef(false);

  const loadData = useCallback(async (status: SelfDevelopmentStatus, request: number) => {
    if (!status.eligible || !status.enabled) return;
    try {
      const values = await Promise.all([
        invoke<unknown>("list_self_development_sources", { projectId }),
        invoke<unknown>("list_self_development_incidents", { projectId }),
      ]);
      const sources = sourceSchema.array().parse(values[0]);
      const incidents = incidentSchema.array().parse(values[1]);
      if (generation.current === request) setResult({ projectId, status, sources, incidents, loading: false, pending: false, error: null });
    } catch {
      if (generation.current === request) setResult({ ...empty(projectId), status, loading: false, error: "Não foi possível consultar os diagnósticos deste dispositivo. Tente atualizar." });
    }
  }, [projectId]);

  const loadStatus = useCallback((request: number) => {
    return invoke<unknown>("get_self_development_status", { projectId }).then(async value => {
      const status = readStatus(value, projectId);
      if (generation.current !== request) return;
      setResult({ ...empty(projectId), status, loading: status.eligible && status.enabled });
      await loadData(status, request);
    }).catch(() => {
      if (generation.current === request) setResult({ ...empty(projectId), loading: false, error: "Não foi possível confirmar o ambiente de desenvolvimento do Jarvis." });
    });
  }, [loadData, projectId]);

  const refresh = useCallback(() => {
    if (busy.current) { refreshPending.current = true; return Promise.resolve(); }
    const request = ++generation.current;
    setResult(current => ({ ...empty(projectId), status: current?.projectId === projectId && current.status?.eligible ? { ...current.status, enabled: false } : null }));
    return loadStatus(request);
  }, [loadStatus, projectId]);

  useEffect(() => {
    busy.current = false;
    refreshPending.current = false;
    const request = ++generation.current;
    void loadStatus(request);
    let active = true;
    const subscription = listen<{ projectId: string }>("self-development-changed", event => {
      if (active && event.payload.projectId === projectId) void refresh();
    }).catch(() => () => {});
    return () => { active = false; generation.current += 1; busy.current = false; refreshPending.current = false; void subscription.then(unlisten => unlisten()); };
  }, [loadStatus, projectId, refresh]);

  const finish = (request: number) => {
    if (generation.current !== request) return;
    busy.current = false;
    if (refreshPending.current) { refreshPending.current = false; void refresh(); }
  };

  const selected = result?.projectId === projectId ? result : null;

  const setEnabled = async (enabled: boolean) => {
    if (!selected?.status?.eligible || busy.current) return false;
    busy.current = true;
    const request = ++generation.current;
    // Revoke visible access immediately, including when a native result is uncertain.
    setResult({ ...empty(projectId), status: { ...selected.status, enabled: false }, loading: false, pending: true });
    try {
      const status = readStatus(await invoke<unknown>("set_self_development_enabled", { projectId, enabled }), projectId);
      if (generation.current !== request) return false;
      if (status.enabled !== enabled) throw new Error("Unconfirmed authorization change");
      setResult({ ...empty(projectId), status, loading: status.enabled });
      await loadData(status, request);
      return generation.current === request;
    } catch {
      if (generation.current === request) setResult({ ...empty(projectId), status: { ...selected.status, enabled: false }, loading: false, error: "Não foi possível confirmar a alteração. Atualize antes de compartilhar diagnósticos." });
      return false;
    } finally { finish(request); }
  };

  const capture = async (conversationId: string, description?: string) => {
    if (!selected?.status?.eligible || !selected.status.enabled || selected.loading || busy.current || !selected.sources.some(source => source.id === conversationId)) return null;
    busy.current = true;
    const request = ++generation.current;
    setResult({ ...selected, pending: true, error: null });
    try {
      const incident = incidentSchema.parse(await invoke<unknown>("capture_self_development_incident", { projectId, conversationId, description: description?.trim() || null }));
      if (generation.current !== request) return null;
      setResult({ ...selected, incidents: [incident, ...selected.incidents.filter(item => item.id !== incident.id)], pending: false, error: null });
      return incident;
    } catch {
      if (generation.current === request) setResult({ ...selected, pending: false, error: "Não foi possível preparar o incidente. A conversa selecionada foi preservada; confirme o resultado antes de tentar novamente." });
      return null;
    } finally { finish(request); }
  };

  const remove = async (incidentId: string) => {
    if (!selected?.status?.eligible || !selected.status.enabled || busy.current || !selected.incidents.some(incident => incident.id === incidentId)) return false;
    busy.current = true;
    const request = ++generation.current;
    setResult({ ...selected, pending: true, error: null });
    try {
      await invoke("delete_self_development_incident", { projectId, incidentId });
      if (generation.current !== request) return false;
      setResult({ ...selected, incidents: selected.incidents.filter(incident => incident.id !== incidentId), pending: false, error: null });
      return true;
    } catch {
      if (generation.current === request) setResult({ ...selected, incidents: [], pending: false, error: "Não foi possível confirmar a remoção. Atualize para verificar os incidentes disponíveis." });
      return false;
    } finally { finish(request); }
  };

  return {
    projectId,
    status: selected?.status ?? null,
    sources: selected?.sources ?? [],
    incidents: selected?.incidents ?? [],
    loading: selected?.loading ?? true,
    pending: selected?.pending ?? false,
    error: selected?.error ?? null,
    refresh,
    setEnabled,
    capture,
    remove,
  };
}

export type SelfDevelopmentController = ReturnType<typeof useSelfDevelopment>;
