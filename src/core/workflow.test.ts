import { expect, it } from "vitest";
import { workflowSchema } from "./workflow";

it("preserves independent stage and request outcomes and blocker diagnostics in native snapshots", () => {
  const handoff = { verdict: "completed", summary: "Planejamento pronto", limitations: [], objective: { status: "partial", remaining: ["Integrar atlas"], reason: "A próxima etapa executa a integração", recoveryAttempts: [], nextAction: "Seguir rota godot" } };
  const result = workflowSchema.parse({ conversationId: "c1", runId: "turn1", revision: 3, flow: "custom", agents: [], stages: [{ number: "1", name: "Planejador", status: "completed", handoff, error: null }] });
  expect(result.stages?.[0].status).toBe("completed");
  expect(result.stages?.[0].handoff).toEqual(handoff);
});
