import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { PendingAuthoring } from "@/core/authoring";

import { AuthoringApprovalDrawer } from "./AuthoringApprovalDrawer";

const agent = {
  id: "a".repeat(32),
  name: "Especialista em acessibilidade",
  description: "Revisa interfaces e entrega evidências WCAG.",
  instructions: "## Objetivo\n\nRevise a interface e liste evidências verificáveis.",
  usage: "mixed" as const,
  capability: "read_only" as const,
  deniedTools: ["bash"],
  model: null,
  appearance: { icon: "shield" as const, color: "cyan" as const },
};

function agentRequest(): PendingAuthoring {
  return {
    turnId: "turn-1", toolId: "tool-1", action: "create", catalogRevision: 4,
    summary: "Adicionar um agente especializado em acessibilidade.",
    target: { kind: "agent", before: null, after: agent }, agentReferences: [],
  };
}

it("reviews an agent proposal and only saves after explicit approval", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  render(<AuthoringApprovalDrawer request={agentRequest()} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("heading", { name: "Revisar alteração no Jarvis" })).toBeVisible();
  expect(within(dialog).getByText("Especialista em acessibilidade")).toBeVisible();
  expect(await within(dialog).findByRole("heading", { name: "Objetivo" })).toBeVisible();
  expect(within(dialog).getByText("Herdar do chat")).toBeVisible();
  expect(within(dialog).getByText("Misto")).toBeVisible();
  expect(answer).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole("button", { name: "Aprovar e salvar" }));
  expect(answer).toHaveBeenCalledWith(true, null);
});

it("shows flow routing with readable agent names and returns a rejection note", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  const first = "b".repeat(32); const second = "c".repeat(32);
  const before = {
    id: "d".repeat(32), name: "Fluxo de revisão", description: "Revisa entregas.",
    entry: first, maxSteps: 2, appearance: { icon: "route" as const, color: "purple" as const },
    steps: [{ id: first, agentId: agent.id, instructions: "Revisar", position: { x: 10, y: 10 }, next: null, onRework: null }],
  };
  const request: PendingAuthoring = {
    turnId: "turn-2", toolId: "tool-2", action: "update", catalogRevision: 7,
    summary: "Adicionar uma etapa de validação ao fluxo.",
    target: { kind: "flow", before, after: { ...before, maxSteps: 4, steps: [
      { ...before.steps[0], next: second },
      { id: second, agentId: agent.id, instructions: "Validar", position: { x: 320, y: 10 }, next: null, onRework: first },
    ] } },
    agentReferences: [{ id: agent.id, name: agent.name }],
  };
  render(<AuthoringApprovalDrawer request={request} owner="Planejador" onAnswer={answer} />);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getAllByText(agent.name)).toHaveLength(2);
  expect(within(dialog).getByText("Etapas")).toBeVisible();
  expect(within(dialog).getByText("Limite")).toBeVisible();
  await user.type(within(dialog).getByLabelText(/Orientação para o agente/), "Troque o nome antes de salvar");
  await user.click(within(dialog).getByRole("button", { name: "Recusar" }));
  expect(answer).toHaveBeenCalledWith(false, "Troque o nome antes de salvar");
});
