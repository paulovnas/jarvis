import { z } from "zod";
import type { TurnOptions } from "./chat";
import { workflowAppearanceSchema } from "./workflow-appearance";

const id = z.string().regex(/^[a-f0-9]{32}$/i);
const builtinRoleSchema = z.enum(["planner", "investigator", "writer", "orchestrator", "designer", "builder", "reviewer"]);
const builtinAgentIdSchema = z.string().regex(/^builtin:(planner|investigator|writer|orchestrator|designer|builder|reviewer)$/);
const agentReferenceIdSchema = z.union([id, builtinAgentIdSchema]);
export const modelChoiceSchema = z.object({ account: z.string(), model: z.string(), reasoning: z.string().nullable() });
export const customAgentSchema = z.object({
  id, name: z.string().min(1).max(100), description: z.string().max(500), instructions: z.string().min(1).max(16000),
  usage: z.enum(["solo", "mixed", "flow_only"]).default("flow_only"),
  capability: z.enum(["read_only", "write_files", "commands"]), model: modelChoiceSchema.nullable(),
  deniedTools: z.array(z.string().min(1).max(128)).max(256).optional(),
  appearance: workflowAppearanceSchema.nullish(),
});
export const workflowStepSchema = z.object({
  id, agentId: agentReferenceIdSchema, instructions: z.string().max(8000),
  position: z.object({ x: z.number(), y: z.number() }), next: id.nullable(), onRework: id.nullable(),
});
export const customFlowSchema = z.object({
  id, name: z.string().min(1).max(100), description: z.string().max(500), entry: id,
  maxSteps: z.number().int().min(1).max(48), steps: z.array(workflowStepSchema).min(1).max(24),
  appearance: workflowAppearanceSchema.nullish(),
});
export const builtinAgentSchema = z.object({
  id: builtinAgentIdSchema, name: z.string(), description: z.string(), instructions: z.string(), role: builtinRoleSchema,
  usage: z.literal("flow_only"), capability: z.enum(["read_only", "write_files", "commands"]),
  appearance: workflowAppearanceSchema, immutable: z.literal(true),
});
export const workflowConnectionSchema = z.object({
  id: z.string(), source: z.string(), target: z.string(), kind: z.literal("delegation"), label: z.string(),
});
export const builtinFlowSchema = z.object({
  id: z.enum(["standard", "designer", "planned", "complete"]), name: z.string(), description: z.string(), entry: z.string(),
  maxSteps: z.number().int().positive(), steps: z.array(workflowStepSchema.extend({ id: z.string(), next: z.null(), onRework: z.null() })),
  connections: z.array(workflowConnectionSchema), appearance: workflowAppearanceSchema, immutable: z.literal(true),
});
export const workflowCatalogSchema = z.object({
  revision: z.number(), agents: z.array(customAgentSchema), flows: z.array(customFlowSchema),
  builtinAgents: z.array(builtinAgentSchema), builtinFlows: z.array(builtinFlowSchema),
});
export type CustomAgent = z.infer<typeof customAgentSchema>;
export type CustomFlow = z.infer<typeof customFlowSchema>;
export type WorkflowStep = z.infer<typeof workflowStepSchema>;
export type BuiltinAgentDefinition = z.infer<typeof builtinAgentSchema>;
export type BuiltinFlowDefinition = z.infer<typeof builtinFlowSchema>;
export type WorkflowConnection = z.infer<typeof workflowConnectionSchema>;
export type FlowAgent = CustomAgent | BuiltinAgentDefinition;
export type WorkflowGraph = CustomFlow | BuiltinFlowDefinition;
export type WorkflowCatalog = z.infer<typeof workflowCatalogSchema>;
export type BuiltinFlow = "standard" | "designer" | "planned" | "complete";
export type FlowSelection = BuiltinFlow | `custom:${string}` | `agent:${string}`;
export type CatalogMutation = { kind: "save_agent"; agent: CustomAgent } | { kind: "save_flow"; flow: CustomFlow } | { kind: "delete_agent" | "delete_flow"; id: string };
export const customId = () => crypto.randomUUID().replace(/-/g, "");
export const flowSelection = (options?: TurnOptions): FlowSelection => options?.workflow === "custom" && options.customAgentId ? `agent:${options.customAgentId}` : options?.workflow === "custom" && options.customWorkflowId ? `custom:${options.customWorkflowId}` : options?.workflow && options.workflow !== "custom" ? options.workflow : "standard";
export function flowOptions(selection: FlowSelection): Pick<TurnOptions, "workflow" | "customWorkflowId" | "customAgentId"> {
  if (selection.startsWith("agent:")) return { workflow: "custom", customAgentId: selection.slice(6) };
  return selection.startsWith("custom:") ? { workflow: "custom", customWorkflowId: selection.slice(7) } : { workflow: selection as BuiltinFlow };
}
export const CAPABILITY_LABELS = { read_only: "Somente leitura", write_files: "Editar arquivos", commands: "Arquivos e comandos" };
export const AGENT_USAGE_LABELS = { solo: "Solo", mixed: "Misto", flow_only: "Somente em fluxos" };
export const isBuiltinAgent = (agent: FlowAgent): agent is BuiltinAgentDefinition => agent.id.startsWith("builtin:");
export const availableFlowAgents = (catalog: WorkflowCatalog): FlowAgent[] => [...catalog.builtinAgents, ...catalog.agents.filter(agent => agent.usage !== "solo")];

export function validateGraph(flow: CustomFlow, agents: FlowAgent[]): string | null {
  if (!flow.name.trim()) return "Dê um nome ao fluxo.";
  if (!flow.steps.length) return "Adicione pelo menos um agente ao canvas.";
  if (flow.steps.length > 24) return "Use até 24 blocos por fluxo.";
  if (flow.maxSteps < flow.steps.length || flow.maxSteps > 48 || !Number.isInteger(flow.maxSteps)) return "O limite deve cobrir todas as etapas e ser de até 48 execuções.";
  const nodes = new Map(flow.steps.map(step => [step.id, step]));
  if (!nodes.has(flow.entry)) return "Escolha o bloco inicial.";
  for (const step of flow.steps) {
    const agent = agents.find(agent => agent.id === step.agentId);
    if (!agent) return "Vincule um agente existente a cada bloco.";
    if (!isBuiltinAgent(agent) && agent.usage === "solo") return "Agentes Solo não podem fazer parte de fluxos. Altere o uso para Misto ou Somente em fluxos.";
    if ([step.next, step.onRework].some(next => next && !nodes.has(next))) return "Remova as conexões para blocos que não existem mais.";
    const seen = new Set<string>();
    let cursor: string | null = step.id;
    while (cursor) {
      if (seen.has(cursor)) return "Uma conexão de conclusão forma um ciclo. Use a saída de correção para retornos.";
      seen.add(cursor); cursor = nodes.get(cursor)?.next ?? null;
    }
  }
  const reachable = new Set<string>();
  const pending = [flow.entry];
  while (pending.length) {
    const current = pending.pop();
    if (!current || reachable.has(current)) continue;
    reachable.add(current);
    const step = nodes.get(current);
    if (step?.next) pending.push(step.next);
    if (step?.onRework) pending.push(step.onRework);
  }
  return reachable.size === nodes.size ? null : "Há blocos desconectados do início. Conecte ou remova esses blocos.";
}
