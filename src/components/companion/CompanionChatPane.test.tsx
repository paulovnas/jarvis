import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { companionChatSchema, type CompanionChat } from "@/core/companion";
import { CompanionChatPane } from "./CompanionChatPane";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@/components/chat/LazyChatMarkdown", () => ({ LazyChatMarkdown: ({ content }: { content: string }) => <p>{content}</p> }));
const call = vi.mocked(invoke);
const events = new Map<string, (payload: unknown) => void>();
const stop = vi.fn();
const options = { account: "codex", model: "gpt-6", reasoning: "high", mode: "build" as const, workflow: "standard" as const, approvalMode: "yolo" as const };
const turn = (user: string, text: string, status: "running" | "completed" = "completed") => ({
  id: "turn-1", createdAt: 1, durationMs: 10, user, options, status, error: null,
  steps: [{ durationMs: 10, text, summary: "Conferindo o pedido", tools: [], usage: null }],
});
const makeChat = (conversationId = "global-chat", project = false): CompanionChat => companionChatSchema.parse({
  conversationId, projectId: project ? "project-1" : null, projectName: project ? "Portal" : null, global: !project, proposal: null,
  chat: { conversationId, revision: 1, turns: [], activeTurnId: null, pendingApproval: null },
});
let globalChat: CompanionChat;
let projectChat: CompanionChat;

describe("Jarvito chat", () => {
  beforeEach(() => {
    globalChat = makeChat(); projectChat = makeChat("project-chat", true);
    events.clear(); stop.mockClear();
    vi.stubGlobal("PointerEvent", MouseEvent);
    vi.mocked(listen).mockReset().mockImplementation(async (name, callback) => {
      events.set(String(name), payload => callback({ event: String(name), id: 1, payload }));
      return stop;
    });
    call.mockReset().mockImplementation(async (command, args) => {
      if (command === "get_companion_chat") return (args as { conversationId?: string } | undefined)?.conversationId ? projectChat : globalChat;
      if (command === "get_companion_conversations") return [{ id: "project-chat", projectId: "project-1", projectName: "Portal", workspaceName: "Trabalho", title: "Ajustar relatório", lastActivityAt: 10 }];
      if (command === "get_companion_models") return [{ provider: "codex", providerKind: "openai-codex", models: [{ value: "codex/gpt-6", label: "GPT-6", reasoningLevels: [], defaultReasoningLevel: null }] }];
      if (command === "send_companion_message") return globalChat;
      if (command === "confirm_companion_project") return (args as { confirmed: boolean }).confirmed ? projectChat : { ...globalChat, proposal: null };
      return true;
    });
  });
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it("opens a passive global chat without a project or opening the main app", async () => {
    render(<CompanionChatPane />);
    expect(await screen.findByText("Oi, eu sou o Jarvito.")).toBeVisible();
    expect(screen.getByRole("combobox", { name: "Conversa do Jarvito" })).toHaveTextContent("Conversar com Jarvito");
    expect(call).toHaveBeenCalledWith("get_companion_chat", undefined);
    expect(call.mock.calls.some(([name]) => name === "send_companion_message" || name === "companion_open_conversation")).toBe(false);
  });

  it("sends a message through the shared runtime, renders the reply and clears the accepted draft", async () => {
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Me ajuda com uma ideia");
    globalChat.chat.turns = [turn("Me ajuda com uma ideia", "Podemos começar pela experiência de quem usa.")];
    globalChat.chat.revision = 2;
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(call).toHaveBeenCalledWith("send_companion_message", { conversationId: null, content: "Me ajuda com uma ideia" });
    expect(await screen.findByText("Podemos começar pela experiência de quem usa.")).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("");
    expect(call).not.toHaveBeenCalledWith("companion_open_conversation", expect.anything());
  });

  it("keeps a failed message editable for a retry", async () => {
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Meu pedido");
    call.mockRejectedValueOnce("A conexão caiu. Tente novamente.");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A conexão caiu");
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Meu pedido");
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeEnabled();
  });

  it("keeps the composer and model control reachable by scrolling when the island is short", async () => {
    const user = userEvent.setup();
    render(<div style={{ height: 138 }}><CompanionChatPane /></div>);
    await screen.findByText("Oi, eu sou o Jarvito.");
    const surface = screen.getByRole("region", { name: "Conversa e controles do Jarvito" });
    const viewport = surface.querySelector<HTMLElement>("[data-slot=scroll-area-viewport]");
    if (!viewport) throw new Error("Missing chat scroll viewport");
    expect(viewport).toContainElement(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }));
    expect(viewport).toContainElement(screen.getByRole("button", { name: "Modelo do Jarvito" }));
    fireEvent.scroll(viewport, { target: { scrollTop: 112 } });
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Minha mensagem");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(call).toHaveBeenCalledWith("send_companion_message", { conversationId: null, content: "Minha mensagem" });
  });

  it("reads an ongoing project conversation without resuming it and sends explicit guidance there", async () => {
    projectChat.chat.turns = [turn("Ajustar o relatório", "Estou conferindo os componentes.", "running")];
    projectChat.chat.activeTurnId = "turn-1";
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.click(screen.getByRole("combobox", { name: "Conversa do Jarvito" }));
    await user.click(await screen.findByRole("option", { name: /Ajustar relatório.*Trabalho.*Portal/ }));
    expect(await screen.findByText("Estou conferindo os componentes.")).toBeVisible();
    expect(call).toHaveBeenCalledWith("get_companion_chat", { conversationId: "project-chat" });
    expect(call.mock.calls.some(([name]) => name === "send_companion_message")).toBe(false);
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Preserve a paleta atual");
    projectChat.chat.queuedMessages = [{ id: "queued-1", content: "Preserve a paleta atual", options }];
    call.mockImplementationOnce(async () => projectChat);
    await user.click(screen.getByRole("button", { name: "Enviar orientação agora" }));
    expect(call).toHaveBeenCalledWith("send_companion_message", { conversationId: "project-chat", content: "Preserve a paleta atual" });
    expect(await screen.findByText("Aguardando envio")).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("companion_open_conversation", expect.anything());
  });

  it("shows a project proposal and switches to the project only after explicit confirmation", async () => {
    globalChat.proposal = { id: "proposal-1", projectId: "project-1", projectName: "Portal", workspaceName: "Trabalho", conversationId: null, reason: "O pedido altera o projeto.", message: "Corrigir a busca do relatório preservando a paleta." };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    expect(await screen.findByRole("region", { name: "Continuar em um projeto" })).toBeVisible();
    expect(screen.getByText("Corrigir a busca do relatório preservando a paleta.")).toBeVisible();
    expect(call.mock.calls.some(([name]) => name === "confirm_companion_project")).toBe(false);
    await user.click(screen.getByRole("button", { name: "Confirmar" }));
    expect(call).toHaveBeenCalledWith("confirm_companion_project", { proposalId: "proposal-1", confirmed: true });
    expect(await screen.findByText("Projeto Portal · conversa compartilhada com o Jarvis")).toBeVisible();
    expect(screen.getByRole("combobox", { name: "Conversa do Jarvito" })).toHaveTextContent("Ajustar relatório");
    expect(call).not.toHaveBeenCalledWith("companion_open_conversation", expect.anything());
  });

  it("allows dismissing the proposed project and stays in the global chat", async () => {
    globalChat.proposal = { id: "proposal-1", projectId: "project-1", projectName: "Portal", workspaceName: "Trabalho", conversationId: "project-chat", reason: "Pedido do projeto.", message: "Revisar o relatório." };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await user.click(await screen.findByRole("button", { name: "Agora não" }));
    expect(call).toHaveBeenCalledWith("confirm_companion_project", { proposalId: "proposal-1", confirmed: false });
    await waitFor(() => expect(screen.queryByRole("region", { name: "Continuar em um projeto" })).not.toBeInTheDocument());
    expect(screen.getByRole("combobox", { name: "Conversa do Jarvito" })).toHaveTextContent("Conversar com Jarvito");
  });

  it("refreshes streaming messages during continuous events and unsubscribes on close", async () => {
    const view = render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    call.mockClear(); vi.useFakeTimers();
    globalChat.chat.turns = [turn("Uma ideia", "Resposta durante o fluxo", "running")];
    const stream = window.setInterval(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }), 50);
    await act(async () => { await vi.advanceTimersByTimeAsync(350); });
    expect(screen.getByText("Resposta durante o fluxo")).toBeVisible();
    expect(call.mock.calls.filter(([name]) => name === "get_companion_chat").length).toBeGreaterThanOrEqual(2);
    window.clearInterval(stream); view.unmount();
    await act(async () => { await Promise.resolve(); });
    expect(stop).toHaveBeenCalled();
  });

  it("submits a pending global question with the actual conversation and turn IDs", async () => {
    globalChat.chat.pendingQuestion = { turnId: "turn-1", toolId: "question-1", deadlineAt: Date.now() + 300_000, questions: [{ id: "topic", question: "Qual assunto?", options: [] }] };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await user.type(await screen.findByRole("textbox", { name: "Sua resposta" }), "Design");
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(call).toHaveBeenCalledWith("companion_answer_question", { conversationId: "global-chat", agentId: null, turnId: "turn-1", toolId: "question-1", response: { cancelled: false, answers: [{ id: "topic", value: "Design" }] } });
    expect(call).toHaveBeenCalledWith("companion_pause_question", { conversationId: "global-chat", agentId: null, turnId: "turn-1", toolId: "question-1" });
  });

  it("dedicates the chat to a question, then restores its draft and model when the question resolves", async () => {
    globalChat.options = { ...options, reasoning: null };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Meu rascunho ainda não enviado");
    globalChat.chat.pendingQuestion = { turnId: "turn-1", toolId: "question-1", questions: [{ id: "scope", question: "Qual escopo usar?", options: [] }] };
    globalChat.chat.revision = 2;
    act(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }));
    await screen.findByRole("heading", { name: "Qual escopo usar?" });
    expect(screen.queryByRole("textbox", { name: "Mensagem para Jarvito" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Modelo do Jarvito" })).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Conversa do Jarvito" })).not.toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Sua resposta" }), "Só o relatório");
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    globalChat.chat.pendingQuestion = null; globalChat.chat.revision = 3;
    act(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }));
    expect(await screen.findByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Meu rascunho ainda não enviado");
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("codex · GPT-6");
  });

  it("reports global questions to the enclosing island with stable source IDs", async () => {
    globalChat.chat.pendingQuestion = { turnId: "turn-1", toolId: "ask-1", questions: [{ id: "name", question: "Qual nome?", options: [] }] };
    const onQuestionChange = vi.fn();
    render(<CompanionChatPane externalQuestions onQuestionChange={onQuestionChange} />);
    await waitFor(() => expect(onQuestionChange).toHaveBeenCalledWith(expect.objectContaining({
      conversationId: "global-chat", agentId: null, request: globalChat.chat.pendingQuestion, requiresConversation: false,
    })));
    expect(screen.queryByRole("region", { name: "Perguntas do Jarvis" })).not.toBeInTheDocument();
  });

  it("rejects mismatched transcript data rather than showing another conversation", async () => {
    call.mockImplementation(async command => command === "get_companion_chat" ? { ...globalChat, chat: { ...globalChat.chat, conversationId: "unexpected" } } : []);
    render(<CompanionChatPane />);
    expect(await screen.findByRole("alert")).toHaveTextContent("não corresponde à conversa selecionada");
    expect(screen.queryByText("Oi, eu sou o Jarvito.")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tentar novamente" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toBeEnabled();
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), { key: "Enter" });
    expect(call.mock.calls.some(([name]) => name === "send_companion_message")).toBe(false);
  });

  it("preserves the unsent draft while hidden and stops reading transcripts", async () => {
    const user = userEvent.setup(); const view = render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Ideia que ainda estou escrevendo");
    call.mockClear(); view.rerender(<CompanionChatPane active={false} />);
    act(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }));
    await act(async () => { await new Promise(resolve => window.setTimeout(resolve, 120)); });
    expect(call.mock.calls.some(([name]) => name === "get_companion_chat")).toBe(false);
    view.rerender(<CompanionChatPane active />);
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_companion_chat", undefined));
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Ideia que ainda estou escrevendo");
  });

  it("stops the selected running conversation without opening the main window", async () => {
    globalChat.chat.turns = [turn("Meu pedido", "Comecei a conferir.", "running")];
    globalChat.chat.activeTurnId = "turn-1";
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Comecei a conferir.");
    globalChat.chat.activeTurnId = null;
    globalChat.chat.turns[0].status = "cancelled";
    call.mockResolvedValueOnce(globalChat);
    await user.click(screen.getByRole("button", { name: "Parar resposta" }));
    expect(call).toHaveBeenCalledWith("stop_companion_chat", { conversationId: "global-chat" });
    await waitFor(() => expect(screen.queryByRole("button", { name: "Parar resposta" })).not.toBeInTheDocument());
    expect(screen.getByText("Meu pedido")).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("companion_open_conversation", expect.anything());
  });

  it("shows the actual provider and model configured for the chat", async () => {
    globalChat.options = { ...options, reasoning: null };
    render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("codex · GPT-6");
    expect(call).toHaveBeenCalledWith("get_companion_models");
    expect(call.mock.calls.some(([name]) => name === "list_provider_accounts" || name === "refresh_claude_runtime")).toBe(false);
  });

  it("replaces an unavailable historical model through the standard picker before sending", async () => {
    globalChat.options = { ...options, model: "retired-model" };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("retired-model · Indisponível");
    await user.click(screen.getByRole("button", { name: "Modelo do Jarvito" }));
    (await screen.findByRole("menuitem", { name: "codex" })).focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "GPT-6" }));
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("GPT-6");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Continue o pedido");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(call).toHaveBeenCalledWith("send_companion_message", { conversationId: null, content: "Continue o pedido", options: { ...options, executor: "jarvis", model: "gpt-6", reasoning: null } });
  });

  it("keeps each conversation's unsent draft separate when switching and collapsing", async () => {
    const user = userEvent.setup(); const view = render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    const input = screen.getByRole("textbox", { name: "Mensagem para Jarvito" });
    await user.type(input, "Rascunho global");
    await user.click(screen.getByRole("combobox", { name: "Conversa do Jarvito" }));
    await user.click(await screen.findByRole("option", { name: /Ajustar relatório.*Trabalho.*Portal/ }));
    await screen.findByText("Projeto Portal · conversa compartilhada com o Jarvis");
    expect(input).toHaveValue("");
    await user.type(input, "Rascunho do projeto");
    view.rerender(<CompanionChatPane active={false} />); view.rerender(<CompanionChatPane active />);
    expect(input).toHaveValue("Rascunho do projeto");
    await user.click(screen.getByRole("combobox", { name: "Conversa do Jarvito" }));
    await user.click(await screen.findByRole("option", { name: /Conversar com Jarvito.*Sem projeto/ }));
    await screen.findByText("Oi, eu sou o Jarvito.");
    expect(input).toHaveValue("Rascunho global");
    expect(call.mock.calls.some(([name]) => name === "send_companion_message")).toBe(false);
  });

  it("does not replace newer streamed text with an older send response", async () => {
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    const old = makeChat(); old.chat.revision = 2; old.chat.turns = [turn("Meu pedido", "Resposta antiga", "running")];
    let finish: ((value: CompanionChat) => void) | undefined;
    call.mockImplementationOnce(() => new Promise<CompanionChat>(resolve => { finish = resolve; }));
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Meu pedido");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    globalChat = makeChat(); globalChat.chat.revision = 3; globalChat.chat.turns = [turn("Meu pedido", "Resposta mais recente", "running")];
    act(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }));
    await screen.findByText("Resposta mais recente");
    await act(async () => { finish?.(old); });
    expect(screen.getByText("Resposta mais recente")).toBeVisible();
    expect(screen.queryByText("Resposta antiga")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("");
  });

  it("does not restore the running state when an older stop response arrives after cancellation", async () => {
    globalChat.chat.activeTurnId = "turn-1"; globalChat.chat.turns = [turn("Meu pedido", "Em andamento", "running")];
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Em andamento");
    const old = makeChat(); old.chat.revision = 2; old.chat.activeTurnId = "turn-1"; old.chat.turns = [turn("Meu pedido", "Em andamento", "running")];
    let finish: ((value: CompanionChat) => void) | undefined;
    call.mockImplementationOnce(() => new Promise<CompanionChat>(resolve => { finish = resolve; }));
    await user.click(screen.getByRole("button", { name: "Parar resposta" }));
    globalChat = makeChat(); globalChat.chat.revision = 3; globalChat.chat.turns = [turn("Meu pedido", "Última mensagem")]; globalChat.chat.turns[0].status = "cancelled";
    act(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }));
    await screen.findByText("Última mensagem");
    await act(async () => { finish?.(old); });
    expect(screen.queryByRole("button", { name: "Parar resposta" })).not.toBeInTheDocument();
    expect(screen.getByText("Última mensagem")).toBeVisible();
  });

  it("refreshes a consumed proposal after confirmation fails and keeps the request without resending", async () => {
    const message = "Corrigir a busca do relatório preservando a paleta.";
    globalChat.proposal = { id: "proposal-1", projectId: "project-1", projectName: "Portal", workspaceName: "Trabalho", conversationId: null, reason: "O pedido altera o projeto.", message };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByRole("region", { name: "Continuar em um projeto" });
    call.mockImplementationOnce(async () => { globalChat.proposal = null; throw "O modelo não está disponível."; });
    await user.click(screen.getByRole("button", { name: "Confirmar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("O modelo não está disponível");
    await waitFor(() => expect(screen.queryByRole("region", { name: "Continuar em um projeto" })).not.toBeInTheDocument());
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue(message);
    act(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }));
    await waitFor(() => expect(call.mock.calls.filter(([name]) => name === "get_companion_chat").length).toBeGreaterThanOrEqual(3));
    expect(screen.getByRole("alert")).toHaveTextContent("Seu pedido foi preservado");
    expect(call.mock.calls.filter(([name]) => name === "confirm_companion_project")).toHaveLength(1);
    expect(call.mock.calls.some(([name]) => name === "send_companion_message")).toBe(false);
  });
});
