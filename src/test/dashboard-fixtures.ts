import type { Bead, ProjectMetrics } from "@/core/dashboard";
export function bead(overrides: Partial<Bead> = {}): Bead {
  return { id: "j0123456789abcdef0123456789abcdef-a", title: "Validar integração", description: "Descrição **real** da tarefa", design: "", acceptance_criteria: "Todos os testes passam", notes: "Contexto persistido", status: "open", priority: 1, issue_type: "task", assignee: "", created_by: "Jarvis", created_at: "2026-09-05T10:00:00Z", updated_at: "2026-09-05T12:00:00Z", closed_at: null, close_reason: "", labels: [], dependencies: [], dependents: [], comment_count: 0, parent: null, ...overrides };
}
export function projectMetrics(): ProjectMetrics {
  return { projectId: "p1", sessions: 2, unavailableSessions: 0, metrics: { turns: 4, inputTokens: 1200, outputTokens: 300, measuredSteps: 5, toolCalls: 6, toolErrors: 1, durationMs: 65_000, compactions: 2, changedFiles: 3, models: { "Conta/gpt-6-astra": 4 }, tools: { ctx_search: 3, beads_show: 2, read: 1 }, days: {} }, recent: [{ id: "c1", title: "Conversa recente", activity: 1788588000, turns: 4 }] };
}
