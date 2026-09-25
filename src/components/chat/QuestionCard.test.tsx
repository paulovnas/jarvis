import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PendingQuestion, QuestionDraft } from "@/core/questions";
import { QuestionCard } from "./QuestionCard";

const request: PendingQuestion = { turnId: "turn1", toolId: "ask1", questions: [
  { id: "place", question: "Onde prefere ficar?", options: [{ label: "Praia", description: "Perto do mar" }, { label: "Montanha" }] },
  { id: "night", question: "O que prefere fazer à noite?", options: [{ label: "Jogar" }, { label: "Ler" }] },
] };
function setup(onAnswer = vi.fn().mockResolvedValue(true)) {
  const drafts = new Map<string, QuestionDraft>();
  const props = { request, onAnswer, onInteract: vi.fn().mockResolvedValue(true), drafts, draftKey: "c1/turn1/ask1" };
  return { ...render(<QuestionCard {...props} />), props, onAnswer, user: userEvent.setup() };
}

describe("Interactive questions", () => {
  afterEach(() => vi.useRealTimers());
  it("offers a clear send action for a single free-text question", () => {
    render(<QuestionCard request={{ turnId: "t", toolId: "a", questions: [{ id: "one", question: "Qual sua preferência?", options: [] }] }} drafts={new Map()} draftKey="one" onAnswer={vi.fn()} onInteract={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Enviar respostas" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Revisar" })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Sua resposta" })).toBeEnabled();
  });
  it("requires an explicit choice and preserves answers while navigating before submitting the batch", async () => {
    const { user, onAnswer } = setup();
    expect(screen.getByRole("button", { name: "Avançar" })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Praia/ })).toHaveAttribute("aria-pressed", "false");
    await user.click(screen.getByRole("button", { name: /Praia/ }));
    await user.click(screen.getByRole("button", { name: "Avançar" }));
    expect(screen.getByText("2 de 2")).toBeVisible();
    await user.type(screen.getByRole("textbox", { name: "Sua resposta" }), "Ficar em casa");
    await user.click(screen.getByRole("button", { name: "Pergunta anterior" }));
    expect(screen.getByRole("button", { name: /Praia/ })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Avançar" }));
    expect(screen.getByRole("textbox")).toHaveValue("Ficar em casa");
    expect(onAnswer).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(onAnswer).toHaveBeenCalledExactlyOnceWith(request, { cancelled: false, answers: [
      { id: "place", value: "Praia", selectedLabel: "Praia" }, { id: "night", value: "Ficar em casa" },
    ] });
  });
  it("supports keyboard selection, free text and Enter without sending unanswered questions", async () => {
    const { user, onAnswer } = setup();
    await user.click(screen.getByRole("button", { name: "Próxima pergunta" }));
    await user.type(screen.getByRole("textbox"), "Ler{Enter}");
    expect(onAnswer).not.toHaveBeenCalled();
    expect(screen.getByText("1 de 2")).toBeVisible();
    screen.getByRole("button", { name: /Praia/ }).focus();
    await user.keyboard("{ArrowDown} ");
    expect(screen.getByRole("button", { name: "Montanha" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Avançar" }));
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(onAnswer).toHaveBeenCalledOnce();
  });
  it("restores a pending draft on remount and replaces a selected option with manual text", async () => {
    const { user, props, unmount, onAnswer } = setup();
    await user.click(screen.getByRole("button", { name: /Praia/ }));
    await user.type(screen.getByRole("textbox"), "Nenhum desses");
    expect(screen.getByRole("button", { name: /Praia/ })).toHaveAttribute("aria-pressed", "false");
    await user.click(screen.getByRole("button", { name: "Avançar" }));
    unmount(); render(<QuestionCard {...props} />);
    expect(screen.getByText("2 de 2")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Jogar" }));
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(onAnswer).toHaveBeenCalledWith(request, { cancelled: false, answers: [
      { id: "place", value: "Nenhum desses" }, { id: "night", value: "Jogar", selectedLabel: "Jogar" },
    ] });
    expect(props.drafts.size).toBe(0);
  });
  it.each(["button", "escape"])("dismisses with %s without treating partial input as an answer", async method => {
    const { user, onAnswer } = setup();
    await user.type(screen.getByRole("textbox"), "Texto parcial");
    if (method === "button") await user.click(screen.getByRole("button", { name: "Cancelar perguntas" }));
    else await user.keyboard("{Escape}");
    expect(onAnswer).toHaveBeenCalledExactlyOnceWith(request, { cancelled: true, answers: [] });
  });
  it("retains answers after failure and blocks duplicate submissions while awaiting confirmation", async () => {
    let resolve!: (accepted: boolean) => void;
    const onAnswer = vi.fn().mockResolvedValueOnce(false).mockImplementation(() => new Promise<boolean>(done => { resolve = done; }));
    const { user } = setup(onAnswer);
    await user.type(screen.getByRole("textbox"), "Praia{Enter}");
    await user.type(screen.getByRole("textbox"), "Jogar{Enter}");
    await waitFor(() => expect(screen.getByRole("button", { name: "Enviar respostas" })).toBeEnabled());
    expect(screen.getByRole("textbox")).toHaveValue("Jogar");
    await user.dblClick(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(onAnswer).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("textbox")).toBeDisabled();
    resolve(true);
  });

  it("shows the backend deadline counting down without submitting a competing UI response", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const onAnswer = vi.fn().mockResolvedValue(true);
    const timed: PendingQuestion = { turnId: "t", toolId: "a", deadlineAt: 3_000, questions: [
      { id: "one", question: "Primeira?", options: [{ label: "A", recommended: true }, { label: "B" }] },
      { id: "two", question: "Segunda?", options: [{ label: "C" }, { label: "D", recommended: true }] },
    ] };
    const onInteract = vi.fn();
    render(<QuestionCard request={timed} drafts={new Map()} draftKey="timed" onAnswer={onAnswer} onInteract={onInteract} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(screen.getByText("2s")).toBeVisible();
    expect(screen.getByText("Recomendada")).toBeVisible();
    await act(async () => { await vi.advanceTimersByTimeAsync(2_100); });
    expect(screen.getByText("Enviando…")).toBeVisible();
    expect(onAnswer).not.toHaveBeenCalled();
    expect(onInteract).not.toHaveBeenCalled();
  });

  it("does not invent an automatic answer when any question lacks a recommendation", () => {
    render(<QuestionCard request={{ turnId: "t", toolId: "a", deadlineAt: Date.now() + 1_000, questions: [
      { id: "one", question: "Escolha?", options: [{ label: "A" }, { label: "B" }] },
    ] }} drafts={new Map()} draftKey="manual" onAnswer={vi.fn()} onInteract={vi.fn()} />);
    expect(screen.queryByText(/resposta recomendada automática/i)).not.toBeInTheDocument();
    expect(screen.queryByText("Recomendada")).not.toBeInTheDocument();
  });

  it.each(["typing", "choice", "keyboard", "navigation"])("pauses on %s, preserves the draft and stays paused after reopening", async method => {
    vi.useFakeTimers();
    const timed = { ...request, deadlineAt: Date.now() + 1_000, questions: request.questions.map(question => ({ ...question, options: question.options.map((option, index) => ({ ...option, recommended: index === 0 })) })) };
    const props = { request: timed, drafts: new Map<string, QuestionDraft>(), draftKey: "timed", onAnswer: vi.fn(), onInteract: vi.fn().mockResolvedValue(true) };
    const { unmount } = render(<QuestionCard {...props} />);
    expect(props.onInteract).not.toHaveBeenCalled();
    if (method === "typing") fireEvent.change(screen.getByRole("textbox"), { target: { value: "Minha preferência" } });
    if (method === "choice") fireEvent.click(screen.getByRole("button", { name: /Montanha/ }));
    if (method === "keyboard") fireEvent.keyDown(screen.getByText("Onde prefere ficar?"), { key: "Tab" });
    if (method === "navigation") fireEvent.click(screen.getByRole("button", { name: "Próxima pergunta" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(1_500); });
    expect(props.onInteract).toHaveBeenCalledExactlyOnceWith(timed);
    expect(screen.getByText("Resposta automática pausada")).toBeVisible();
    expect(screen.queryByText("Enviando…")).not.toBeInTheDocument();
    expect(props.onAnswer).not.toHaveBeenCalled();
    unmount(); render(<QuestionCard {...props} />);
    expect(screen.getByText("Resposta automática pausada")).toBeVisible();
    if (method === "typing") expect(screen.getByRole("textbox")).toHaveValue("Minha preferência");
    if (method === "choice") expect(screen.getByRole("button", { name: /Montanha/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("keeps the countdown visible after a failed pause and retries on the next interaction", async () => {
    const user = userEvent.setup();
    const onInteract = vi.fn().mockResolvedValueOnce(false).mockResolvedValue(true);
    const timed = { ...request, deadlineAt: Date.now() + 30_000, questions: request.questions.map(question => ({ ...question, options: question.options.map((option, index) => ({ ...option, recommended: index === 0 })) })) };
    render(<QuestionCard request={timed} drafts={new Map()} draftKey="retry" onAnswer={vi.fn()} onInteract={onInteract} />);
    await user.keyboard("{ArrowRight}");
    expect(screen.queryByText("Resposta automática pausada")).not.toBeInTheDocument();
    expect(await screen.findByText(/Resposta recomendada automática em/)).toBeInTheDocument();
    await user.keyboard("{ArrowRight}");
    expect(await screen.findByText("Resposta automática pausada")).toBeVisible();
    expect(onInteract).toHaveBeenCalledTimes(2);
  });
});
