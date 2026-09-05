import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ToolCallItem } from "./types";
import { ToolCallCard } from "./ToolCallCard";

const tool: ToolCallItem = { id: "ask1", name: "ask_user", status: "completed", args: { questions: [
  { id: "a", question: "Onde prefere ficar?", options: [{ label: "Praia" }] },
  { id: "b", question: "O que prefere fazer?", options: [] },
] }, output: JSON.stringify({ cancelled: false, answers: [{ id: "a", value: "Praia", selectedLabel: "Praia" }, { id: "b", value: "Jogar" }] }) };
describe("Question history", () => {
  it("starts collapsed and displays readable questions and answers without tool parameters", async () => {
    const user = userEvent.setup(); render(<ToolCallCard tool={tool} />);
    const trigger = screen.getByRole("button", { name: "Feitas 2 perguntas" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Jogar")).not.toBeInTheDocument();
    await user.click(trigger);
    expect(screen.getByText("Onde prefere ficar?")).toBeVisible();
    expect(screen.getByText("Praia")).toBeVisible();
    expect(screen.getByText("Jogar")).toBeVisible();
    expect(screen.queryByText("Parâmetros")).not.toBeInTheDocument();
    expect(screen.queryByText(/selectedLabel/)).not.toBeInTheDocument();
  });
  it("shows unanswered cancellation and tolerates invalid historic arguments", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ToolCallCard tool={{ ...tool, output: JSON.stringify({ cancelled: true, answers: [] }) }} />);
    await user.click(screen.getByRole("button", { name: "Feitas 2 perguntas" }));
    expect(screen.getAllByText("Não respondida")).toHaveLength(2);
    rerender(<ToolCallCard tool={{ ...tool, args: { questions: "invalid" }, output: "invalid" }} />);
    expect(screen.getByText("Perguntas não disponíveis")).toBeVisible();
  });
});
