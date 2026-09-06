import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
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
