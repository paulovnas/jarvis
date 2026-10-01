import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PendingQuestion, QuestionDraft } from "@/core/questions";
import { CompanionQuestion, type CompanionQuestionContext } from "./CompanionQuestion";

const request: PendingQuestion = { turnId: "turn-1", toolId: "tool-1", questions: [
  { id: "scope", question: "Qual escopo devemos implementar?", options: [{ label: "Completo", description: "Inclui todas as telas e a revisão final.", recommended: true }, { label: "Apenas o relatório", description: "Preserva as outras telas." }] },
  { id: "name", question: "Como chamar o relatório?", options: [] },
] };
const context: CompanionQuestionContext = { conversationId: "chat-1", agentId: "designer-1", projectName: "Portal", title: "Revisar relatório", requiresConversation: false, request };

function setup(overrides?: Partial<CompanionQuestionContext>) {
  const onAnswer = vi.fn().mockResolvedValue(true);
  const onInteract = vi.fn().mockResolvedValue(true);
  const onOpenConversation = vi.fn();
  const drafts = new Map<string, QuestionDraft>();
  const props = { context: { ...context, ...overrides }, onAnswer, onInteract, onOpenConversation, drafts };
  return { ...props, view: render(<CompanionQuestion {...props} />) };
}

describe("Jarvito dedicated questions", () => {
  afterEach(() => vi.useRealTimers());

  it("shows one question, readable descriptions and fixed controls without a chat composer", async () => {
    const user = userEvent.setup(); const { onAnswer } = setup();
    expect(screen.getByRole("heading", { name: request.questions[0].question })).toBeVisible();
    expect(screen.queryByText(request.questions[1].question)).not.toBeInTheDocument();
    expect(screen.getByText("Inclui todas as telas e a revisão final.")).toBeVisible();
    expect(screen.getByText("1 de 2")).toBeVisible();
    expect(screen.getByRole("button", { name: "Próxima pergunta" })).toBeDisabled();
    expect(screen.queryByRole("textbox", { name: "Mensagem para Jarvito" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Apenas o relatório/ }));
    await user.click(screen.getByRole("button", { name: "Próxima pergunta" }));
    expect(screen.queryByText(request.questions[0].question)).not.toBeInTheDocument();
    expect(screen.getByText("2 de 2")).toBeVisible();
    await user.type(screen.getByRole("textbox", { name: "Sua resposta" }), "Resumo mensal");
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(onAnswer).toHaveBeenCalledWith(request, { cancelled: false, answers: [{ id: "scope", value: "Apenas o relatório", selectedLabel: "Apenas o relatório" }, { id: "name", value: "Resumo mensal" }] });
    const footer = screen.getByRole("button", { name: "Enviar respostas" }).closest<HTMLElement>(".companion-question-footer");
    expect(footer).not.toBeNull();
    expect(screen.getByRole("region", { name: "Opções e resposta" })).not.toContainElement(footer);
    expect(screen.getByRole("region", { name: "Pergunta e ações" })).toContainElement(footer);
  });

  it("pauses native automatic answers on interaction and preserves the draft between stages and remounts", async () => {
    const user = userEvent.setup();
    const deadlineRequest = { ...request, deadlineAt: Date.now() + 60_000 };
    const result = setup({ request: deadlineRequest });
    await user.click(screen.getByRole("button", { name: /Completo/ }));
    expect(result.onInteract).toHaveBeenCalledTimes(1);
    expect(result.onInteract).toHaveBeenCalledWith(deadlineRequest);
    expect(await screen.findByText("Resposta automática pausada")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Próxima pergunta" }));
    await user.type(screen.getByRole("textbox", { name: "Sua resposta" }), "Minha escolha");
    expect(result.onInteract).toHaveBeenCalledTimes(1);
    result.view.unmount();
    render(<CompanionQuestion {...result} />);
    expect(screen.getByRole("textbox", { name: "Sua resposta" })).toHaveValue("Minha escolha");
    await user.click(screen.getByRole("button", { name: "Pergunta anterior" }));
    expect(screen.getByRole("button", { name: /Completo/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("Resposta automática pausada")).toBeVisible();
  });

  it("preserves an answer after a failed submission and permits explicit retry", async () => {
    const user = userEvent.setup();
    const single = { ...request, questions: [request.questions[1]] };
    const result = setup({ request: single }); result.onAnswer.mockResolvedValueOnce(false);
    await user.type(screen.getByRole("textbox", { name: "Sua resposta" }), "Relatório mensal");
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(screen.getByRole("textbox", { name: "Sua resposta" })).toHaveValue("Relatório mensal");
    expect(screen.getByRole("button", { name: "Enviar respostas" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(result.onAnswer).toHaveBeenCalledTimes(2);
  });

  it("keeps an answer and its actions reachable through the same scrolling surface in a short island", async () => {
    const user = userEvent.setup();
    const onAnswer = vi.fn().mockResolvedValue(true);
    render(<div style={{ height: 138, display: "flex", flexDirection: "column" }}><CompanionQuestion context={{ ...context, request: { ...request, questions: [request.questions[1]] } }} drafts={new Map()} onAnswer={onAnswer} onInteract={vi.fn().mockResolvedValue(true)} onOpenConversation={vi.fn()} /></div>);
    const surface = screen.getByRole("region", { name: "Pergunta e ações" });
    const viewport = surface.querySelector<HTMLElement>("[data-slot=scroll-area-viewport]");
    if (!viewport) throw new Error("Missing question scroll viewport");
    expect(viewport).toContainElement(screen.getByRole("textbox", { name: "Sua resposta" }));
    expect(viewport).toContainElement(screen.getByRole("button", { name: "Revisar respostas" }));
    fireEvent.scroll(viewport, { target: { scrollTop: 120 } });
    await user.type(screen.getByRole("textbox", { name: "Sua resposta" }), "Relatório mensal");
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(onAnswer).toHaveBeenCalledWith(expect.objectContaining({ toolId: request.toolId }), { cancelled: false, answers: [{ id: "name", value: "Relatório mensal" }] });
  });

  it("uses an explicit main-chat action for visual questions and complex approvals", async () => {
    const user = userEvent.setup();
    const visual: PendingQuestion = { ...request, questions: [{ id: "color", question: "Qual paleta?", options: [{ label: "Azul", preview: { type: "palette", colors: ["#000000", "#ffffff"], sample: "Portal" } }] }] };
    const result = setup({ request: visual });
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Azul/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continuar no Jarvis" }));
    expect(result.onOpenConversation).toHaveBeenCalledOnce();
    result.view.rerender(<CompanionQuestion {...result} context={{ ...context, request: null, requiresConversation: true }} />);
    expect(screen.getByRole("region", { name: "Decisão no Jarvis" })).toBeVisible();
    expect(result.onAnswer).not.toHaveBeenCalled();
  });

  it("leaves automatic submission to the native runtime and allows cancelling explicitly", async () => {
    vi.useFakeTimers();
    const single = { ...request, deadlineAt: Date.now() + 10_000, questions: [request.questions[0]] };
    const result = setup({ request: single });
    await act(async () => { await vi.advanceTimersByTimeAsync(15_000); });
    expect(result.onAnswer).not.toHaveBeenCalled();
    vi.useRealTimers();
    const user = userEvent.setup();
    await user.click(within(screen.getByRole("region", { name: "Perguntas do Jarvis" })).getByRole("button", { name: "Cancelar" }));
    expect(result.onAnswer).toHaveBeenCalledWith(single, { cancelled: true, answers: [] });
  });
});
