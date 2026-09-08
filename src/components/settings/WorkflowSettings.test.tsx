import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { WorkflowSettings } from "./WorkflowSettings";
import { useWorkflowCatalog } from "@/hooks/use-workflow-catalog";
import { customAgent, customCatalog } from "@/test/workflow-fixtures";

vi.mock("@/hooks/use-workflow-catalog", () => ({ useWorkflowCatalog: vi.fn() }));
vi.mock("./workflow/WorkflowCanvas", () => ({ default: () => <div aria-label="Canvas do fluxo" /> }));
const mutate = vi.fn().mockResolvedValue(true);
beforeEach(() => { mutate.mockClear(); vi.mocked(useWorkflowCatalog).mockReturnValue({ data: customCatalog, error: null, saving: false, refresh: vi.fn(), mutate }); });

it("separates immutable built-in flow and agent cards from custom management", async () => {
  const user = userEvent.setup(); render(<WorkflowSettings accounts={[]} />);
  const jarvis = screen.getByRole("region", { name: "Fluxos Jarvis" });
  expect(within(jarvis).getAllByRole("button")).toHaveLength(4);
  expect(within(jarvis).queryByRole("button", { name: /Excluir|Editar/ })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Editar Meu fluxo" })).toBeVisible();
  await user.click(screen.getByRole("tab", { name: "Agentes" }));
  expect(within(screen.getByRole("region", { name: "Agentes Jarvis" })).getAllByRole("button")).toHaveLength(7);
  expect(screen.getByRole("button", { name: "Editar Analista próprio" })).toBeVisible();
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
