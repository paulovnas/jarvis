import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { CustomFlowEditor } from "./CustomFlowEditor";
import { customAgent, customFlow } from "@/test/workflow-fixtures";

vi.mock("./WorkflowCanvas", () => ({ default: () => <div aria-label="Canvas do fluxo" /> }));
it("adds blocks, blocks saving disconnected graphs and connects them through keyboard-accessible controls", async () => {
  const user = userEvent.setup(); const save = vi.fn().mockResolvedValue(true);
  render(<CustomFlowEditor initial={customFlow} agents={[customAgent]} saving={false} creating={false} onSave={save} onClose={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Adicionar bloco" }));
  expect(screen.getByRole("button", { name: "Salvar fluxo" })).toBeDisabled();
  expect(screen.getByRole("status")).toHaveTextContent("desconectados");
  const editBlock = screen.getByRole("combobox", { name: "Editar bloco" });
  await user.click(editBlock);
  await user.click(await screen.findByRole("option", { name: "1. Analista próprio" }));
  // Select retains hidden options while focused; wait for its observable close
  // and focus restoration before opening a different popup.
  await waitFor(() => {
    expect(editBlock).toHaveTextContent("1. Analista próprio");
    expect(editBlock).toHaveAttribute("aria-expanded", "false");
    expect(editBlock).toHaveFocus();
  });
  await user.click(screen.getByRole("combobox", { name: "Ao concluir" }));
  await user.click(await screen.findByRole("option", { name: "2. Analista próprio" }));
  expect(screen.getByRole("button", { name: "Salvar fluxo" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Cor Ciano" }));
  await user.click(screen.getByRole("button", { name: "Ícone Foguete" }));
  await user.click(screen.getByRole("button", { name: "Salvar fluxo" }));
  await waitFor(() => expect(save).toHaveBeenCalledOnce());
  const saved = save.mock.calls[0][0] as typeof customFlow;
  expect(saved.steps).toHaveLength(2);
  expect(saved.steps[0].next).toBe(saved.steps[1].id);
  expect(saved.entry).toBe(customFlow.entry);
  expect(saved.appearance).toEqual({ color: "cyan", icon: "rocket" });
});

it("asks before discarding a changed canvas definition", async () => {
  const user = userEvent.setup(); const close = vi.fn();
  render(<CustomFlowEditor initial={customFlow} agents={[customAgent]} saving={false} creating={false} onSave={vi.fn()} onClose={close} />);
  await user.type(screen.getByLabelText("Nome"), " alterado");
  await user.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(close).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Descartar" }));
  expect(close).toHaveBeenCalledOnce();
});
