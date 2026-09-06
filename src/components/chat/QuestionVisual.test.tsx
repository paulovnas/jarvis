import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { QuestionCard } from "./QuestionCard";
import { QuestionHistory } from "./QuestionHistory";
import { questionRequestSchema } from "@/core/questions";

it("shows comparable visual choices, enlarges without answering, and records the chosen preview", async () => {
  const user = userEvent.setup(); const answer = vi.fn().mockResolvedValue(true);
  const request = { turnId: "turn", toolId: "ask", ...questionRequestSchema.parse({ questions: [{ id: "layout", question: "Qual composição?", options: [
    { label: "Menu lateral", preview: { type: "wireframe", elements: [{ label: "Menu", x: 0, y: 0, width: 20, height: 100 }, { label: "Conteúdo", x: 20, y: 0, width: 80, height: 100 }] } },
    { label: "Azul", preview: { type: "palette", colors: ["#21252b", "#61afef"], sample: "Projeto" } },
  ] }] }) };
  const view = render(<QuestionCard request={request} drafts={new Map()} draftKey="q" onAnswer={answer} />);
  expect(screen.getByRole("img", { name: "Wireframe: Menu lateral" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Ampliar Menu lateral" }));
  expect(screen.getByRole("dialog", { name: "Menu lateral" })).toBeVisible(); expect(answer).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(answer).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: /^Menu lateral/ }));
  await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
  const output = { cancelled: false, answers: [{ id: "layout", value: "Menu lateral", selectedLabel: "Menu lateral" }] };
  expect(answer).toHaveBeenCalledWith(request, output);
  view.unmount();
  render(<QuestionHistory tool={{ id: "ask", name: "ask_user", args: { questions: request.questions }, status: "completed", output: JSON.stringify(output) }} />);
  expect(screen.queryByRole("img")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Feita 1 pergunta/ }));
  expect(screen.getByRole("img", { name: "Wireframe: Menu lateral" })).toBeVisible();
});
