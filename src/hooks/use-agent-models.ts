import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import { toast } from "sonner";
import { libraryError } from "@/core/library";
import type { Workflow, WorkflowAgent } from "@/core/workflow";

const choiceSchema = z.object({ account: z.string(), model: z.string(), reasoning: z.string().nullable() });
const configSchema = z.record(z.string(), choiceSchema);
export type ModelChoice = z.infer<typeof choiceSchema>;
export type AgentModelConfig = z.infer<typeof configSchema>;
export function useAgentModels() {
  const [data, setData] = useState<AgentModelConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const flight = useRef(false);
  const version = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++version.current;
    try { const config = configSchema.parse(await invoke("get_agent_models")); if (request === version.current) { setData(config); setError(null); } }
    catch (cause) { if (request === version.current) setError(libraryError(cause, "Não foi possível carregar os modelos dos agentes.")); }
  }, []);
  useEffect(() => {
    let active = true; let dispose: (() => void) | undefined;
    void listen("agent-models:changed", () => { if (active) void refresh(); }).then(stop => { if (!active) { stop(); return; } dispose = stop; void refresh(); }).catch(() => { if (active) void refresh(); });
    return () => { active = false; version.current += 1; dispose?.(); };
  }, [refresh]);
  const save = async (flow: Workflow, role: WorkflowAgent["role"], choice: ModelChoice) => {
    if (flight.current) return false;
    flight.current = true; setSaving(true); ++version.current;
    try { const config = configSchema.parse(await invoke("set_agent_model", { flow, role, choice })); ++version.current; setData(config); setError(null); return true; }
    catch (cause) { toast.error(libraryError(cause, "Não foi possível salvar o modelo do agente.")); return false; }
    finally { flight.current = false; setSaving(false); }
  };
  return { data, error, saving, save, refresh };
}
export type AgentModelsController = ReturnType<typeof useAgentModels>;
