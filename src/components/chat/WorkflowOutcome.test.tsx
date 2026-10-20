import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { WorkflowOutcome } from "./WorkflowOutcome";

it("does not invent a technical cause or recovery attempts for an undocumented blocker", () => {
  render(<WorkflowOutcome result={{ status: "blocked", handoff: null, error: null }} />);
  expect(screen.getByText("Motivo do bloqueio")).toBeVisible();
  expect(screen.getByText(/O agente não registrou a causa/)).toBeVisible();
  expect(screen.getByText("Nenhuma tentativa registrada.")).toBeVisible();
  expect(screen.getByText(/Consultar o histórico do agente/)).toBeVisible();
});

it("keeps a successful stage quiet when the whole request has no remaining work", () => {
  const { container } = render(<WorkflowOutcome result={{ status: "completed", error: null, handoff: { verdict: "completed", summary: "Entregue", objective: { status: "completed", remaining: [], reason: "", recoveryAttempts: [], nextAction: "" } } }} />);
  expect(container).toBeEmptyDOMElement();
});
