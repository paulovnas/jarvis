import { z } from "zod";
import { turnOptionsSchema, type AgentTool } from "./chat";
import { pendingQuestionSchema } from "./questions";

export type Workflow = "standard" | "planned" | "complete";
export const FLOW_LABELS: Record<Workflow, string> = { standard: "Padrão", planned: "Planejado", complete: "Completo" };
export const ROLE_LABELS = { planner: "Planejador", investigator: "Investigador", writer: "Redator", orchestrator: "Orquestrador", designer: "Designer", builder: "Construtor", reviewer: "Revisor" };
export const ROLE_COLORS = { planner: "#c678dd", investigator: "#56b6c2", writer: "#e08a78", orchestrator: "#e5c07b", designer: "#c678dd", builder: "#61afef", reviewer: "#98c379" };
export const STATUS_LABELS = { queued: "Na fila", running: "Executando", waiting: "Aguardando", completed: "Concluído", blocked: "Bloqueado", failed: "Falhou", cancelled: "Cancelado", interrupted: "Interrompido" };
export const agentCardSchema = z.object({
  id: z.string(), parentId: z.string().nullable(), role: z.enum(["planner", "investigator", "writer", "orchestrator", "designer", "builder", "reviewer"]),
  title: z.string(), status: z.enum(["queued", "running", "waiting", "completed", "blocked", "failed", "cancelled", "interrupted"]),
  createdAt: z.number(), updatedAt: z.number(), attempts: z.number(), options: turnOptionsSchema, beadId: z.string().nullable(),
  handoff: z.object({ verdict: z.enum(["completed", "approved", "rework", "blocked"]), summary: z.string() }).nullable(),
  error: z.string().nullable(), activeTurnId: z.string().nullable(),
  pendingApproval: z.object({ id: z.string(), name: z.string(), args: z.record(z.string(), z.unknown()), status: z.enum(["pending", "running", "completed", "error"]), output: z.string(), durationMs: z.number() }).nullable() satisfies z.ZodType<AgentTool | null>,
  pendingQuestion: pendingQuestionSchema.nullable(),
});
export const workflowSchema = z.object({ conversationId: z.string(), revision: z.number(), flow: z.enum(["standard", "planned", "complete"]), agents: z.array(agentCardSchema) });
export type WorkflowAgent = z.infer<typeof agentCardSchema>;
export type WorkflowSnapshot = z.infer<typeof workflowSchema>;
export const activeAgent = (agent: WorkflowAgent) => ["queued", "running", "waiting"].includes(agent.status);
