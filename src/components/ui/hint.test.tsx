import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { Hint } from "./hint";

it("shows accessible shadcn help without a native browser title", async () => {
  render(<TooltipProvider delay={0}><Hint content="Solicita aprovação ao concluir"><button type="button">Validação manual</button></Hint></TooltipProvider>);
  const trigger = screen.getByRole("button", { name: "Validação manual" });
  expect(trigger).not.toHaveAttribute("title");
  fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
  fireEvent.mouseEnter(trigger);
  fireEvent.mouseMove(trigger);
  expect(await screen.findByRole("tooltip")).toHaveTextContent("Solicita aprovação ao concluir");
});

it("keeps the original element when there is no useful hint", () => {
  render(<Hint content={undefined}><span>Sem ajuda</span></Hint>);
  expect(screen.getByText("Sem ajuda")).toBeInTheDocument();
  expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
});
