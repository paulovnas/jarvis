import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

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

afterEach(() => vi.restoreAllMocks());

it("only shows repeated text when the trigger is actually truncated", async () => {
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(80);
  vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(80);
  const view = render(<TooltipProvider delay={0}><Hint content="Conversa completa" whenTruncated><span>Conversa completa</span></Hint></TooltipProvider>);
  const trigger = screen.getByText("Conversa completa");
  fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
  fireEvent.mouseEnter(trigger);
  expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();

  vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(140);
  view.rerender(<TooltipProvider delay={0}><Hint content="Conversa completa" whenTruncated><span>Conversa completa</span></Hint></TooltipProvider>);
  fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
  fireEvent.mouseEnter(trigger);
  fireEvent.mouseMove(trigger);
  expect(await screen.findByRole("tooltip")).toHaveTextContent("Conversa completa");
});
