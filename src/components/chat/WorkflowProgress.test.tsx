import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { savedTurn } from "@/test/chat-fixtures";
import { AssistantMessageTurn } from "./AssistantMessageTurn";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";

const planner = { thinking: "", commentary: "Etapa 1 · RPG/Planejador em execução.", tools: [] };
const director = { thinking: "", commentary: "Etapa 2 · RPG/Diretor em execução.", tools: [] };
const artist = { thinking: "", commentary: "Etapa 3 · RPG/Artista Técnico 3D em execução.", tools: [] };

it("shows a real earlier blocker and its next action instead of inferring success from the next stage", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 5, steps: [planner, director], workflowStages: [{
    number: "1", name: "RPG/Planejador", status: "blocked", error: null,
    handoff: { verdict: "blocked", summary: "Inspeção pendente", objective: {
      status: "blocked", remaining: ["Conferir o atlas"], reason: "O modelo de visão não respondeu.",
      recoveryAttempts: ["Tentada nova conexão sem repetir a geração."], nextAction: "Selecionar outro modelo de visão e retomar.",
    } },
  }] }} />);
  await user.click(screen.getByRole("button", { name: /Etapa 2.*RPG\/Diretor/ }));
  const row = within(screen.getByRole("list", { name: "Histórico do fluxo" })).getAllByRole("listitem")[0];
  expect(within(row).getByText("Bloqueada")).toBeVisible();
  expect(within(row).getByText("O modelo de visão não respondeu.", { exact: false })).toBeVisible();
  expect(within(row).getByText("Selecionar outro modelo de visão e retomar.", { exact: false })).toBeVisible();
  expect(within(row).queryByText("Concluída")).not.toBeInTheDocument();
});

it("groups stage announcements around the current stage and reveals their history on demand", async () => {
  const user = userEvent.setup();
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 4, steps: [planner, director] }} />);
  const progress = screen.getByRole("region", { name: "Progresso do fluxo" });
  const disclosure = within(progress).getByRole("button", { name: /Etapa 2.*RPG\/Diretor/ });
  expect(disclosure).toHaveAttribute("aria-expanded", "false");
  expect(disclosure).toHaveTextContent("Em execução");
  expect(screen.queryByText("RPG/Planejador")).not.toBeInTheDocument();
  expect(screen.queryByText(planner.commentary)).not.toBeInTheDocument();

  await user.click(disclosure);
  rerender(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 5, steps: [planner, director, artist] }} />);
  expect(screen.getAllByRole("region", { name: "Progresso do fluxo" })).toHaveLength(1);
  expect(disclosure).toHaveAttribute("aria-expanded", "true");
  expect(disclosure).toHaveAccessibleName(/Etapa 3.*RPG\/Artista Técnico 3D/);
  const rows = within(screen.getByRole("list", { name: "Histórico do fluxo" })).getAllByRole("listitem");
  expect(rows.map(row => within(row).getByText(/RPG\//).textContent)).toEqual(["RPG/Planejador", "RPG/Diretor", "RPG/Artista Técnico 3D"]);
  expect(within(rows[0]).getByText("Encerrada")).toBeVisible();
  expect(within(rows[1]).getByText("Encerrada")).toBeVisible();
  expect(rows[2]).toHaveAttribute("aria-current", "step");
  expect(within(rows[2]).getByText("Em execução")).toBeVisible();
});

it.each([
  "Etapa 1 · RPG/Planejador em execução.\n\nVou examinar os arquivos antes de continuar.",
  "Vou iniciar a Etapa 1 · RPG/Planejador em execução.",
  "Etapa 1 · RPG/Planejador em execução. Confirme os arquivos.",
])("keeps ordinary assistant prose containing a stage announcement intact: %s", async commentary => {
  render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 1, steps: [{ ...planner, commentary }] }} />);
  expect(screen.queryByRole("region", { name: "Progresso do fluxo" })).not.toBeInTheDocument();
  expect(await screen.findByText(/Etapa 1 · RPG\/Planejador em execução\./)).toBeVisible();
  if (commentary.includes("Vou examinar")) expect(await screen.findByText("Vou examinar os arquivos antes de continuar.")).toBeVisible();
});

it("keeps the latest workflow stage active while ordinary assistant observations follow it", async () => {
  render(<AssistantWorkCollapse isStreaming work={{ durationSeconds: 2, steps: [planner, { thinking: "", commentary: "Vou inspecionar o cenário.", tools: [] }] }} />);
  const disclosure = within(screen.getByRole("region", { name: "Progresso do fluxo" })).getByRole("button", { name: /Etapa 1.*RPG\/Planejador/ });
  expect(disclosure).toHaveAttribute("aria-current", "step");
  expect(disclosure).toHaveTextContent("Em execução");
  expect(await screen.findByText("Vou inspecionar o cenário.")).toBeVisible();
});

it("preserves tools, ordinary observations and user messages between separate stage groups", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse isStreaming work={{
    durationSeconds: 5,
    steps: [
      { ...planner, tools: [{ id: "read-plan", name: "read", status: "completed", args: { path: "plan.md" }, output: "Plano" }] },
      { thinking: "", commentary: "Vou manter a direção combinada.", tools: [] },
      director,
    ],
    auxiliaryMessages: [{ id: "stage-direction", content: "Use o cenário atual", options: savedTurn().options, afterStep: 2 }],
  }} />);
  const groups = screen.getAllByRole("region", { name: "Progresso do fluxo" });
  expect(groups).toHaveLength(2);
  await user.click(within(groups[0]).getByRole("button", { name: /Etapa 1/ }));
  await user.click(within(groups[0]).getByRole("button", { name: /Leu e pesquisou arquivos/ }));
  const tool = screen.getByTestId("tool-call-read-plan");
  const observation = await screen.findByText("Vou manter a direção combinada.");
  const message = screen.getByTestId("user-message-stage-direction");
  expect(tool.compareDocumentPosition(observation) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(observation.compareDocumentPosition(message) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(message.compareDocumentPosition(groups[1]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it.each([
  { status: "completed" as const, error: undefined, state: "Encerrada" },
  { status: undefined, error: undefined, state: "Encerrada" },
  { status: undefined, error: { title: "Falha na execução", message: "O provedor encerrou a conexão." }, state: "Não concluída" },
])("shows the final stage as $state using the turn outcome", async ({ status, error, state }) => {
  const user = userEvent.setup();
  render(<AssistantMessageTurn message={{
    id: "workflow-turn", role: "assistant", content: "", timestamp: "12:00", error,
    work: { status, durationSeconds: 6, steps: [planner, director] },
  }} />);
  await user.click(screen.getByRole("button", { name: /Trabalhou por 6s/ }));
  const progress = screen.getByRole("region", { name: "Progresso do fluxo" });
  const disclosure = within(progress).getByRole("button", { name: /Etapa 2.*RPG\/Diretor/ });
  expect(disclosure).toHaveTextContent(state);
  expect(within(progress).queryByText("Em execução")).not.toBeInTheDocument();
  await user.click(disclosure);
  const rows = within(screen.getByRole("list", { name: "Histórico do fluxo" })).getAllByRole("listitem");
  expect(within(rows[0]).getByText("Encerrada")).toBeVisible();
  expect(within(rows[1]).getByText(state)).toBeVisible();
  expect(rows[1]).not.toHaveAttribute("aria-current");
});
