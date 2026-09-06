import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { workflowSchema, type WorkflowSnapshot } from "@/core/workflow";
import { readChat, type ChatSnapshot } from "@/core/chat";
import { libraryError } from "@/core/library";

// Subscribe before the initial read; coalesce streaming invalidations without polling.
function useWorkflowQuery<T>(conversationId: string | null, agentId: string | undefined, parse: (value: unknown, id: string) => T) {
  const [result, setResult] = useState<{ key: string; data: T | null; error: string | null } | null>(null);
  const [attempt, setAttempt] = useState(0);
  const key = `${conversationId ?? ""}/${agentId ?? ""}`;
  useEffect(() => {
    if (!conversationId) return;
    let active = true, running = false, dirty = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const stops: (() => void)[] = [];
    const refresh = async () => {
      if (!active) return;
      if (running) { dirty = true; return; }
      running = true; dirty = false;
      try {
        const value = await invoke(agentId ? "get_workflow_transcript" : "get_workflow", { conversationId, ...(agentId ? { agentId } : {}) });
        const data = parse(value, agentId === "main" || !agentId ? conversationId : agentId);
        if (active) setResult({ key, data, error: null });
      } catch (cause) { if (active) setResult(current => ({ key, data: current?.key === key ? current.data : null, error: libraryError(cause, "Não foi possível carregar o fluxo.") })); }
      finally { running = false; if (active && dirty) schedule(); }
    };
    const schedule = () => { if (timer) return; timer = setTimeout(() => { timer = undefined; void refresh(); }, 150); };
    void Promise.all(["workflow:changed", "agent:updated"].map(event => listen<{ conversationId: string }>(event, event => { if (active && event.payload.conversationId === conversationId) schedule(); }).then(stop => { if (active) stops.push(stop); else stop(); }))).then(() => { if (active) void refresh(); }).catch(cause => { if (active) setResult({ key, data: null, error: libraryError(cause, "Não foi possível acompanhar o fluxo.") }); });
    return () => { active = false; if (timer) clearTimeout(timer); stops.forEach(stop => stop()); };
  }, [conversationId, agentId, key, parse, attempt]);
  return { data: result?.key === key ? result.data : null, error: result?.key === key ? result.error : null, loading: !!conversationId && result?.key !== key, retry: () => setAttempt(value => value + 1) };
}
function parseWorkflow(value: unknown, id: string): WorkflowSnapshot | null {
  if (value === null) return null;
  const data = workflowSchema.parse(value);
  if (data.conversationId !== id) throw new Error("O fluxo não corresponde à conversa.");
  return data;
}
export function useWorkflow(id: string | null) { return useWorkflowQuery(id, undefined, parseWorkflow); }
export function useWorkflowTranscript(id: string | null, agentId: string) { return useWorkflowQuery<ChatSnapshot>(id, agentId, readChat); }
export type WorkflowController = ReturnType<typeof useWorkflow>;
