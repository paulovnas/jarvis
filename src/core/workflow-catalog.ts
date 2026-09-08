import { z } from "zod";
import type { TurnOptions } from "./chat";
import { workflowAppearanceSchema } from "./workflow-appearance";

const id = z.string().regex(/^[a-f0-9]{32}$/i);
export const modelChoiceSchema = z.object({ account: z.string(), model: z.string(), reasoning: z.string().nullable() });
export const customAgentSchema = z.object({
  id, name: z.string().min(1).max(100), description: z.string().max(500), instructions: z.string().min(1).max(16000),
  capability: z.enum(["read_only", "write_files", "commands"]), model: modelChoiceSchema.nullable(),
  deniedTools: z.array(z.string().min(1).max(128)).max(256).optional(),
  appearance: workflowAppearanceSchema.nullish(),
});
export const workflowStepSchema = z.object({
  id, agentId: id, instructions: z.string().max(8000),
  position: z.object({ x: z.number(), y: z.number() }), next: id.nullable(), onRework: id.nullable(),
});
export const customFlowSchema = z.object({
  id, name: z.string().min(1).max(100), description: z.string().max(500), entry: id,
  maxSteps: z.number().int().min(1).max(48), steps: z.array(workflowStepSchema).min(1).max(24),
  appearance: workflowAppearanceSchema.nullish(),
});
export const workflowCatalogSchema = z.object({ revision: z.number(), agents: z.array(customAgentSchema), flows: z.array(customFlowSchema) });
export type CustomAgent = z.infer<typeof customAgentSchema>;
export type CustomFlow = z.infer<typeof customFlowSchema>;
export type WorkflowStep = z.infer<typeof workflowStepSchema>;
export type WorkflowCatalog = z.infer<typeof workflowCatalogSchema>;
export type BuiltinFlow = "standard" | "designer" | "planned" | "complete";
export type FlowSelection = BuiltinFlow | `custom:${string}`;
export type CatalogMutation = { kind: "save_agent"; agent: CustomAgent } | { kind: "save_flow"; flow: CustomFlow } | { kind: "delete_agent" | "delete_flow"; id: string };
export const customId = () => crypto.randomUUID().replace(/-/g, "");
export const flowSelection = (options?: TurnOptions): FlowSelection => options?.workflow === "custom" && options.customWorkflowId ? `custom:${options.customWorkflowId}` : options?.workflow && options.workflow !== "custom" ? options.workflow : "standard";
export function flowOptions(selection: FlowSelection): Pick<TurnOptions, "workflow" | "customWorkflowId"> {
  return selection.startsWith("custom:") ? { workflow: "custom", customWorkflowId: selection.slice(7) } : { workflow: selection as BuiltinFlow };
}
export const CAPABILITY_LABELS = { read_only: "Somente leitura", write_files: "Editar arquivos", commands: "Arquivos e comandos" };

export function validateGraph(flow: CustomFlow, agents: CustomAgent[]): string | null {
  if (!flow.name.trim()) return "Dê um nome ao fluxo.";
  if (!flow.steps.length) return "Adicione pelo menos um agente ao canvas.";
  if (flow.steps.length > 24) return "Use até 24 blocos por fluxo.";
  if (flow.maxSteps < flow.steps.length || flow.maxSteps > 48 || !Number.isInteger(flow.maxSteps)) return "O limite deve cobrir todas as etapas e ser de até 48 execuções.";
  const nodes = new Map(flow.steps.map(step => [step.id, step]));
  if (!nodes.has(flow.entry)) return "Escolha o bloco inicial.";
  for (const step of flow.steps) {
    if (!agents.some(agent => agent.id === step.agentId)) return "Vincule um agente existente a cada bloco.";
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
