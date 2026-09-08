import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { customFlow } from "@/test/workflow-fixtures";
import { FlowPicker } from "./FlowPicker";

it("oferece os quatro fluxos com descrição e seleciona sem alterar o modelo", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  expect(await screen.findAllByRole("menuitem")).toHaveLength(4);
  expect(screen.getByRole("group", { name: "Fluxos" })).toHaveClass("grid-cols-1");
  expect(screen.getByText("Referências, direção visual e interfaces.")).toBeVisible();
  await user.click(screen.getByRole("menuitem", { name: "Designer" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("designer");
});
it("bloqueia a troca durante execução", async () => {
  render(<FlowPicker value="complete" onChange={vi.fn()} disabled />);
  expect(screen.getByRole("button", { name: "Selecionar fluxo" })).toBeDisabled();
});

it("selects a saved custom flow without changing the four Jarvis choices", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} customFlows={[customFlow]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  expect(await screen.findAllByRole("menuitem")).toHaveLength(5);
  await user.click(screen.getByRole("menuitem", { name: "Meu fluxo" }));
  expect(change).toHaveBeenCalledExactlyOnceWith(`custom:${customFlow.id}`);
});

it("uses the saved flow icon and color both in the composer and menu", async () => {
  const user = userEvent.setup();
  render(<FlowPicker value={`custom:${customFlow.id}`} onChange={vi.fn()} customFlows={[{ ...customFlow, appearance: { color: "green", icon: "rocket" } }]} />);
  const trigger = screen.getByRole("button", { name: "Selecionar fluxo" });
  expect(trigger.querySelector("svg.lucide-rocket")).toHaveStyle({ color: "var(--color-onedark-green)" });
  await user.click(trigger);
  expect((await screen.findByRole("menuitem", { name: "Meu fluxo" })).querySelector("svg.lucide-rocket")).toBeInTheDocument();
});
