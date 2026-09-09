import { z } from "zod";

const count = z.number().int().nonnegative();
const counts = z.record(z.string(), count);
export const efficiencySchema = z.object({
  contextSearches: count.default(0),
  loopSteers: count.default(0), loopAvoidedCalls: count.default(0),
  cacheReadTokens: count.default(0), cacheWriteTokens: count.default(0),
  cacheReadInputTokens: count.default(0), cacheReadRequests: count.default(0), cacheWriteRequests: count.default(0),
  auxiliaryRequests: count.default(0), auxiliaryInputTokens: count.default(0), auxiliaryOutputTokens: count.default(0),
  indexedOutputs: count.default(0), originalBytes: count.default(0), retainedBytes: count.default(0),
});
export const emptyEfficiency = efficiencySchema.parse({});
export const projectMetricsSchema = z.object({
  projectId: z.string(), sessions: count, unavailableSessions: count,
  metrics: z.object({ turns: count, inputTokens: count, outputTokens: count, measuredSteps: count,
    efficiency: efficiencySchema.optional(),
    toolCalls: count, toolErrors: count, durationMs: count, compactions: count, changedFiles: count,
    models: counts, tools: counts, days: counts }),
  recent: z.array(z.object({ id: z.string(), title: z.string(), activity: count, turns: count })),
});
const relation = z.object({ id: z.string(), title: z.string(), status: z.string(), dependency_type: z.string() });
export const beadSchema = z.object({
  id: z.string().min(1), title: z.string(), description: z.string(), design: z.string(), acceptance_criteria: z.string(), notes: z.string(),
  status: z.string(), priority: count, issue_type: z.string(), assignee: z.string(), created_by: z.string(),
  created_at: z.string(), updated_at: z.string(), closed_at: z.string().nullable(), close_reason: z.string(),
  labels: z.array(z.string()), dependencies: z.array(relation), dependents: z.array(relation), comment_count: count,
  parent: z.string().nullable(),
});
export const commentSchema = z.object({ id: z.string().min(1), author: z.string(), text: z.string(), created_at: z.string() });
export const detailSchema = z.object({ issue: beadSchema, comments: z.array(commentSchema) });
export const boardSchema = z.array(beadSchema);
export type ProjectMetrics = z.infer<typeof projectMetricsSchema>;
export type Bead = z.infer<typeof beadSchema>;
export type BeadDetail = z.infer<typeof detailSchema>;

export const statuses = [
  { id: "open", label: "Aberto", color: "#969eac" },
  { id: "in_progress", label: "Em progresso", color: "#61afef" },
  { id: "blocked", label: "Bloqueado", color: "#e06c75" },
  { id: "deferred", label: "Adiado", color: "#e5c07b" },
  { id: "closed", label: "Fechado", color: "#98c379" },
  { id: "pinned", label: "Fixado", color: "#c678dd" },
  { id: "hooked", label: "Vinculado", color: "#56b6c2" },
];
export function statusFor(id: string) { return statuses.find(status => status.id === id) ?? { id, label: id, color: "#969eac" }; }
export function typeName(type: string) { return ({ epic: "Épico", task: "Tarefa", feature: "História", bug: "Bug", chore: "Manutenção", decision: "Decisão" } as Record<string, string>)[type] ?? type; }
export function shortId(id: string, projectName = "jarvis") {
  const match = /^j[a-f0-9]{32}-([a-f0-9]+)(.*)$/.exec(id);
  if (!match) return id;
  const prefix = projectName.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "jarvis";
  return `${prefix}-${match[1].slice(0, 6)}${match[1].length > 6 ? "…" : ""}${match[2]}`;
}
export function actorName(actor: string) { return /^jarvis-[a-f0-9]{32}$/.test(actor) ? "Jarvis" : actor; }
export const number = (value: number) => new Intl.NumberFormat("pt-BR", { notation: value >= 10_000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(value);
export function date(value: string | number, time = false) {
  const parsed = new Date(typeof value === "number" ? value * 1000 : value);
  return Number.isNaN(parsed.getTime()) ? "—" : new Intl.DateTimeFormat("pt-BR", { day: "2-digit", month: "short", ...(time ? { hour: "2-digit", minute: "2-digit" } as const : {}) }).format(parsed);
}
export function dashboardError(error: unknown) {
  if (typeof error === "object" && error && "message" in error && typeof error.message === "string" && !(error instanceof z.ZodError)) return error.message;
  return "Não foi possível carregar os dados do projeto.";
}
