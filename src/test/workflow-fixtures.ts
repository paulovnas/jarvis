import type { CustomAgent, CustomFlow, WorkflowCatalog } from "@/core/workflow-catalog";

export const customAgent: CustomAgent = { id: "a".repeat(32), name: "Analista próprio", description: "Investiga com evidências", instructions: "Leia os arquivos e apresente evidências.", capability: "read_only", model: null };
export const customFlow: CustomFlow = { id: "b".repeat(32), name: "Meu fluxo", description: "Análise personalizada", entry: "c".repeat(32), maxSteps: 12, steps: [{ id: "c".repeat(32), agentId: customAgent.id, instructions: "", position: { x: 40, y: 40 }, next: null, onRework: null }] };
export const customCatalog: WorkflowCatalog = { revision: 2, agents: [customAgent], flows: [customFlow] };
