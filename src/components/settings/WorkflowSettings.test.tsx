import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { WorkflowSettings } from "./WorkflowSettings";
import { useWorkflowCatalog } from "@/hooks/use-workflow-catalog";
import { customAgent, customCatalog } from "@/test/workflow-fixtures";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@/hooks/use-workflow-catalog", () => ({ useWorkflowCatalog: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("./workflow/WorkflowCanvas", () => ({ default: () => <div aria-label="Canvas do fluxo" /> }));
vi.mock("./AgentSettings", () => ({ AgentSettings: () => <div aria-label="Modelos do fluxo" /> }));
const mutate = vi.fn().mockResolvedValue(true);
beforeEach(() => { mutate.mockClear(); vi.mocked(useWorkflowCatalog).mockReturnValue({ data: customCatalog, error: null, saving: false, refresh: vi.fn(), mutate }); });

it("separates immutable built-in flow and agent cards from custom management", async () => {
  const user = userEvent.setup(); render(<WorkflowSettings accounts={[]} />);
  expect(screen.getByRole("tablist", { name: "Fluxos" })).toHaveAttribute("aria-orientation", "horizontal");
  const jarvis = screen.getByRole("region", { name: "Fluxos Jarvis" });
  expect(within(jarvis).getAllByRole("button")).toHaveLength(6);
  expect(within(jarvis).queryByRole("button", { name: /Excluir|Editar/ })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Editar Meu fluxo" })).toBeVisible();
  await user.click(screen.getByRole("tab", { name: "Agentes" }));
  const nativeAgents = screen.getByRole("region", { name: "Agentes Jarvis" });
  expect(within(nativeAgents).getAllByRole("button")).toHaveLength(11);
  expect(within(nativeAgents).getByRole("button", { name: "Ver agente GitHub" })).toHaveTextContent("Misto");
  expect(within(nativeAgents).getByRole("button", { name: "Ver agente Gerador de vídeos" })).toHaveTextContent("Misto");
  expect(within(nativeAgents).getByRole("button", { name: "Ver agente Gerador de vídeos" })).toHaveTextContent("Ajuda a definir público, mensagem e roteiro.");
  expect(within(nativeAgents).getByRole("button", { name: "Ver agente Gerador de imagens" })).toHaveTextContent("Misto");
  expect(screen.getByRole("button", { name: "Editar Analista próprio" })).toBeVisible();
});

it("opens the dedicated pixel artist instructions through its native custom agent contract", async () => {
  vi.mocked(invoke).mockResolvedValueOnce([{ title: "Pixel art", content: "Opera a suíte compartilhada desta conversa." }]);
  const user = userEvent.setup();
  render(<WorkflowSettings accounts={[]} initialTab="agents" />);
  await user.click(screen.getByRole("button", { name: "Ver agente Artista de pixel art" }));
  const instructions = await screen.findByRole("article", { name: "Instruções de Artista de pixel art" });
  expect(await within(instructions).findByText("Opera a suíte compartilhada desta conversa.")).toBeVisible();
  expect(invoke).toHaveBeenCalledWith("get_agent_instructions", { flow: "custom", role: "pixel_art" });
});

it("edits custom instructions and saves only the selected definition with its original revision", async () => {
  const user = userEvent.setup(); render(<WorkflowSettings accounts={[]} />);
  await user.click(screen.getByRole("tab", { name: "Agentes" }));
  await user.click(screen.getByRole("button", { name: "Editar Analista próprio" }));
  await user.clear(screen.getByLabelText("Instruções do agente"));
  await user.type(screen.getByLabelText("Instruções do agente"), "Examine os testes.");
  await user.click(screen.getByRole("button", { name: "Cor Roxo" }));
  await user.click(screen.getByRole("button", { name: "Ícone Cérebro" }));
  expect(screen.getByRole("button", { name: "Cor Roxo" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("button", { name: "Ícone Cérebro" })).toHaveAttribute("aria-pressed", "true");
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  await waitFor(() => expect(mutate).toHaveBeenCalledWith({ kind: "save_agent", agent: { ...customAgent, instructions: "Examine os testes.", appearance: { color: "purple", icon: "brain" } } }, 2));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("opens the real native definition in the shared read-only canvas", async () => {
  const user = userEvent.setup(); render(<WorkflowSettings accounts={[]} />);
  await user.click(screen.getByRole("button", { name: "Ver fluxo Designer" }));
  expect(await screen.findByRole("dialog")).toBeVisible();
  expect(await screen.findByLabelText("Canvas do fluxo")).toBeVisible();
  expect(screen.getByText("Somente leitura")).toBeVisible();
  expect(screen.getByText(/topologia executada pelo Jarvis/i)).toBeVisible();
  expect(screen.getByLabelText("Modelos do fluxo")).toBeVisible();
});

it("closes a built-in flow with its persistent footer action", async () => {
  const user = userEvent.setup(); render(<WorkflowSettings accounts={[]} />);
  await user.click(screen.getByRole("button", { name: "Ver fluxo Designer" }));
  const dialog = await screen.findByRole("dialog");
  const footer = within(dialog).getByRole("group", { name: "Ações do fluxo" });
  const close = within(footer).getByRole("button", { name: "Fechar" });
  expect(close).toBeVisible();
  await user.click(close);
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(mutate).not.toHaveBeenCalled();
});

it("requires a concrete delete confirmation and preserves the editor after save failure", async () => {
  const user = userEvent.setup(); render(<WorkflowSettings accounts={[]} />);
  await user.click(screen.getByRole("button", { name: "Excluir Meu fluxo" }));
  expect(mutate).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(mutate).not.toHaveBeenCalled();
  await user.click(screen.getByRole("tab", { name: "Agentes" }));
  await user.click(screen.getByRole("button", { name: "Editar Analista próprio" }));
  mutate.mockResolvedValueOnce(false);
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  expect(await screen.findByRole("dialog")).toBeVisible();
});
