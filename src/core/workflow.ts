import { z } from "zod";
import { turnOptionsSchema, type AgentTool } from "./chat";
import { pendingQuestionSchema } from "./questions";
import { workflowAppearanceSchema } from "./workflow-appearance";
import { pendingAuthoringSchema } from "./authoring";

export type Workflow = "standard" | "designer" | "planned" | "complete" | "custom";
export const FLOW_LABELS: Record<Workflow, string> = { standard: "Padrão", designer: "Designer", planned: "Planejado", complete: "Completo", custom: "Customizado" };
export const rootRole = (flow: Workflow) => flow === "custom" ? "custom" : flow === "standard" ? "builder" : flow === "designer" ? "designer" : "planner";
export const ROLE_LABELS = { planner: "Planejador", investigator: "Investigador", writer: "Redator", orchestrator: "Orquestrador", designer: "Designer", builder: "Construtor", reviewer: "Revisor", custom: "Customizado" };
export const ROLE_COLORS = { planner: "#c678dd", investigator: "#56b6c2", writer: "#e08a78", orchestrator: "#e5c07b", designer: "#ef8fba", builder: "#61afef", reviewer: "#98c379", custom: "#969eac" };
export const STATUS_LABELS = { queued: "Na fila", running: "Executando", waiting: "Aguardando", completed: "Concluído", blocked: "Bloqueado", failed: "Falhou", cancelled: "Cancelado", interrupted: "Interrompido" };
export const agentCardSchema = z.object({
  id: z.string(), parentId: z.string().nullable(), role: z.enum(["planner", "investigator", "writer", "orchestrator", "designer", "builder", "reviewer", "custom"]),
  title: z.string(), status: z.enum(["queued", "running", "waiting", "completed", "blocked", "failed", "cancelled", "interrupted"]),
  createdAt: z.number(), updatedAt: z.number(), startedAt: z.number(), durationMs: z.number().nonnegative(), currentThought: z.string().nullable(), attempts: z.number(), options: turnOptionsSchema, beadId: z.string().nullable(),
  handoff: z.object({ verdict: z.enum(["completed", "approved", "rework", "blocked"]), summary: z.string() }).nullable(),
  error: z.string().nullable(), activeTurnId: z.string().nullable(),
  pendingApproval: z.object({ id: z.string(), name: z.string(), args: z.record(z.string(), z.unknown()), status: z.enum(["pending", "running", "completed", "error"]), output: z.string(), durationMs: z.number() }).nullable() satisfies z.ZodType<AgentTool | null>,
  pendingQuestion: pendingQuestionSchema.nullable(),
  pendingAuthoring: pendingAuthoringSchema.nullable().optional(),
  identity: z.object({ name: z.string(), appearance: workflowAppearanceSchema.nullish() }).nullish(),
});
export const validationItemSchema = z.object({ id: z.string(), title: z.string(), steps: z.array(z.string()), expected: z.string(), decision: z.enum(["pending", "approved", "rejected"]), reason: z.string().nullable() });
export const validationSchema = z.object({ id: z.string(), flow: z.enum(["planned", "complete"]), runId: z.string(), epicIds: z.array(z.string()), items: z.array(validationItemSchema), submitted: z.boolean(), stale: z.boolean(), createdAt: z.number() });
export type ValidationBatch = z.infer<typeof validationSchema>;
export type ValidationItem = z.infer<typeof validationItemSchema>;
export const workflowRecoverySchema = z.object({
  runId: z.string(),
  affectedAgents: z.number().int().positive(),
  uncertainActions: z.array(z.object({ agentId: z.string(), agentTitle: z.string(), tool: z.string() })),
});
export const workflowSchema = z.object({ conversationId: z.string(), revision: z.number(), flow: z.enum(["standard", "designer", "planned", "complete", "custom"]), agents: z.array(agentCardSchema), validation: validationSchema.nullable().optional(), recovery: workflowRecoverySchema.nullable().optional() });
export type WorkflowAgent = z.infer<typeof agentCardSchema>;
export type WorkflowSnapshot = z.infer<typeof workflowSchema>;
export type WorkflowRecovery = z.infer<typeof workflowRecoverySchema>;
export const activeAgent = (agent: WorkflowAgent) => ["queued", "running", "waiting"].includes(agent.status);
