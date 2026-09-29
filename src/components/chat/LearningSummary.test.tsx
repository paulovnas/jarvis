import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import type { ProjectLesson } from "@/core/project-learning";
import { savedTurn } from "@/test/chat-fixtures";
import { LearningSummary } from "./LearningSummary";
import { TurnBody } from "./Transcript";

const lesson: ProjectLesson = {
  id: "select", scope: "frontend", content: "Mostre o label selecionado no Select.",
  topics: ["select"], check: "Conferir o texto após carregar as opções.", status: "active", origin: "feedback",
  evidence: [{ conversationId: "c1", messageId: "turn1", excerpt: "Sempre mostre o label no seletor", createdAt: 1 }],
  revision: 1, updatedAt: 1,
};

it("keeps learned content collapsed until opened and distinguishes suggestions", async () => {
  const user = userEvent.setup();
  render(<LearningSummary lessons={[lesson, { ...lesson, id: "spacing", content: "Preserve o espaçamento dos botões.", scope: ".", status: "suggested" }]} />);
  const trigger = screen.getByRole("button", { name: /Aprendizados registrados.*2/ });
  expect(trigger).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByText(lesson.content)).not.toBeInTheDocument();
  trigger.focus();
  await user.keyboard("{Enter}");
  const list = screen.getByRole("list", { name: "Aprendizados desta interação" });
  expect(within(list).getAllByRole("listitem")).toHaveLength(2);
  expect(within(list).getByText(lesson.content)).toBeVisible();
  expect(within(list).getAllByText(lesson.check, { exact: false })).toHaveLength(2);
  expect(within(list).getByText("frontend")).toBeVisible();
  expect(within(list).getByText("Projeto inteiro")).toBeVisible();
  expect(within(list).getByText("Ativo")).toBeVisible();
  expect(within(list).getByText("Sugestão")).toBeVisible();
  expect(within(list).getByText(/Ainda não usado pelos agentes/)).toBeVisible();
  await user.click(trigger);
  expect(trigger).toHaveAttribute("aria-expanded", "false");
});

it("does not add a disclosure when no lesson was saved", () => {
  const { container } = render(<LearningSummary lessons={[]} />);
  expect(container).toBeEmptyDOMElement();
});

it("places lessons after the answer, including live feedback, without mixing conversations or turns", async () => {
  const user = userEvent.setup();
  const turn = savedTurn();
  turn.auxiliaryMessages = [{ id: "feedback", content: "Sempre use cursor pointer.", options: turn.options, parts: [] }];
  const lessons = [lesson,
    { ...lesson, id: "pointer", content: "Use cursor pointer nos controles.", evidence: [{ ...lesson.evidence[0], messageId: "feedback" }] },
    { ...lesson, id: "other-chat", content: "Outra conversa", evidence: [{ ...lesson.evidence[0], conversationId: "c2" }] },
    { ...lesson, id: "other-turn", content: "Outro turno", evidence: [{ ...lesson.evidence[0], messageId: "turn2" }] },
    { ...lesson, id: "imported", content: "Importado", origin: "imported" as const, evidence: [] },
  ];
  const { rerender } = render(<TurnBody turn={turn} conversationId="c1" lessons={[]} />);
  expect(screen.queryByRole("button", { name: /Aprendizados registrados/ })).not.toBeInTheDocument();
  rerender(<TurnBody turn={turn} conversationId="c1" lessons={lessons} />);
  const trigger = screen.getByRole("button", { name: /Aprendizados registrados.*2/ });
  expect(screen.getByRole("group", { name: "Ações da resposta" }).compareDocumentPosition(trigger) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getByRole("button", { name: /Trabalhou por/ })).toHaveAttribute("aria-expanded", "false");
  await user.click(trigger);
  expect(screen.getByText(lesson.content)).toBeVisible();
  expect(screen.getByText("Use cursor pointer nos controles.")).toBeVisible();
  expect(screen.queryByText("Outra conversa")).not.toBeInTheDocument();
  expect(screen.queryByText("Outro turno")).not.toBeInTheDocument();
  expect(screen.queryByText("Importado")).not.toBeInTheDocument();
});
