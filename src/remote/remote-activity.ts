import { ROLE_LABELS } from "@/core/workflow";
import { reasoningPreview } from "@/components/chat/reasoning-preview";
import type { RemoteChat, RemoteLibrary, RemoteRuntime } from "./client";

export type ActivityStatus = "running" | "waiting" | "failed" | "completed" | "idle" | "reconnecting" | "compacting";

export function aggregateActivity(library: RemoteLibrary, conversationIds: string[]) {
  const ids = new Set(conversationIds);
  const result = { total: ids.size, running: 0, waiting: 0, failed: 0, completed: 0, status: "idle" as ActivityStatus };
  for (const runtime of library.runtime) {
    if (!ids.delete(runtime.conversationId)) continue;
    const status = runtimeStatus(runtime);
    if (status !== "idle") result[status]++;
  }
  result.status = result.waiting ? "waiting" : result.running ? "running" : result.failed ? "failed" : result.completed ? "completed" : "idle";
  return result;
}

function runtimeStatus(runtime: RemoteRuntime): "waiting" | "running" | "failed" | "completed" | "idle" {
  if (runtime.attention.length || runtime.status === "waiting") return "waiting";
  if (runtime.compacting || runtime.activeTurnId || runtime.status === "running") return "running";
  if (runtime.status === "failed" || runtime.status === "blocked") return "failed";
  return runtime.status === "completed" ? "completed" : "idle";
}

export function currentActivity(bundle: RemoteChat): { status: ActivityStatus; label: string; detail: string | null; agent: string | null; tool: string | null } {
  const { chat, workflow } = bundle;
  const turn = chat.turns.find(item => item.id === chat.activeTurnId) ?? chat.turns[chat.turns.length - 1];
  const agents = workflow?.agents ?? [];
  const worker = agents.filter(item => ["running", "waiting", "queued"].includes(item.status))
    .sort((a, b) => Number(b.id !== "main") - Number(a.id !== "main") || b.updatedAt - a.updatedAt)[0];
  const agent = worker ? worker.identity?.name ?? ROLE_LABELS[worker.role] : null;
  const step = turn?.steps[turn.steps.length - 1];
  const rootIsCurrent = !worker || worker.id === "main";
  const tool = rootIsCurrent ? [...(step?.tools ?? [])].reverse().find(item => item.status === "running") : undefined;
  const waiting = chat.pendingQuestion || chat.pendingApproval || chat.pendingAuthoring || agents.some(item => item.pendingQuestion || item.pendingApproval || item.pendingAuthoring)
    || workflow?.validation && !workflow.validation.submitted && !workflow.validation.stale;
  const thought = worker?.currentThought?.trim() || (!rootIsCurrent ? worker?.title : step?.summary.trim());
  const detail = thought ? reasoningPreview(thought) || null : null;
  if (waiting) return { status: "waiting", label: "Precisa de você", detail: "Há uma pergunta ou decisão aguardando sua resposta.", agent, tool: null };
  if (chat.compacting) return { status: "compacting", label: "Organizando contexto", detail, agent, tool: null };
  if (chat.activeTurnId || worker) {
    if (rootIsCurrent && step?.retry) return { status: "reconnecting", label: "Reconectando ao provedor", detail: step.retry.message, agent, tool: null };
    return { status: "running", label: tool ? "Executando ferramenta" : "Em execução", detail, agent, tool: tool?.name ?? null };
  }
  if (turn?.status === "error") return { status: "failed", label: "Falhou", detail: turn.error?.message ?? null, agent: null, tool: null };
  const failed = agents.filter(item => ["failed", "blocked"].includes(item.status) && (!turn || Math.max(item.updatedAt, item.startedAt) >= turn.createdAt))
    .sort((a, b) => b.updatedAt - a.updatedAt)[0];
  if (failed) return { status: "failed", label: failed.status === "blocked" ? "Bloqueado" : "Falhou", detail: failed.error ?? failed.handoff?.summary ?? null, agent: failed.identity?.name ?? ROLE_LABELS[failed.role], tool: null };
  if (turn?.status === "completed") return { status: "completed", label: "Concluído", detail: null, agent: null, tool: null };
  return { status: "idle", label: turn ? "Interrompido" : "Pronto para conversar", detail: null, agent: null, tool: null };
}
