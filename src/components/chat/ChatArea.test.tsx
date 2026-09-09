import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyLibrary, populatedLibrary } from "@/test/library-fixtures";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import type { LibrarySnapshot } from "@/core/library";
import type { ChatSnapshot } from "@/core/chat";
import { useChat } from "@/hooks/use-chat";
import { ChatArea } from "./ChatArea";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const listeners = new Set<EventCallback<unknown>>();
function TestChat({ library = populatedLibrary(), connected = true }: { library?: LibrarySnapshot; connected?: boolean }) {
  const chat = useChat(library.selection.conversationId);
  return <ChatArea library={library} chat={chat} modelGroups={connected ? [{ provider:"Codex", models:[{ value:"openai-codex-pessoal/model",label:"Modelo real",reasoningLevels:["medium"],defaultReasoningLevel:"medium" }] }] : []} />;
}
async function update(snapshot: ChatSnapshot) {
  await act(async () => { for (const handler of listeners) handler({ event: "agent:updated", id: 1, payload: snapshot }); });
}
describe("Persistent live conversation", () => {
  beforeEach(() => {
    listeners.clear(); call.mockReset().mockResolvedValue(emptyChat());
    vi.mocked(listen).mockImplementation(async (_name, callback) => {
      listeners.add(callback); return () => { listeners.delete(callback); };
    });
  });
  it("submits during execution and removes queued messages through the native commands", async () => {
    const user = userEvent.setup();
    const turn = { ...savedTurn(), status: "running" as const };
    const running = { ...emptyChat(), turns: [turn], activeTurnId: turn.id };
    const queued = { id: "q1", content: "Depois rode os testes", options: turn.options };
    call.mockImplementation(async command => {
      if (command === "start_agent_turn") return { ...running, revision: 2, queuedMessages: [queued] };
      if (command === "remove_queued_message") return { message: queued, snapshot: { ...running, revision: 3, queuedMessages: [] } };
      return running;
    });
    render(<TestChat />); await screen.findByRole("textbox");
    await user.type(screen.getByRole("textbox"), "Depois rode os testes{Enter}");
    expect(await screen.findByRole("region", { name: "Mensagens agendadas" })).toHaveTextContent("Depois rode os testes");
    await user.click(screen.getByRole("button", { name: "Retirar mensagem 1 e editar" }));
    expect(call).toHaveBeenCalledWith("remove_queued_message", { conversationId: "c1", messageId: "q1" });
    expect(screen.queryByRole("region", { name: "Mensagens agendadas" })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveTextContent("Depois rode os testes");
  }, 15000);

  it("locks and restores typing during automatic compaction events", async () => {
    const user = userEvent.setup(); render(<TestChat />);
    await screen.findByRole("textbox");
    await user.type(screen.getByRole("textbox"), "Rascunho preservado");
    const context = { tokens: 900, limit: 1000, estimated: false, compacting: true, compactions: 0 };
    await update({ ...emptyChat(), revision: 2, context });
    expect(screen.getByRole("textbox")).toHaveAttribute("contenteditable", "false");
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
    await update({ ...emptyChat(), revision: 3, context: { ...context, tokens: 100, compacting: false, compactions: 1 } });
    expect(screen.getByRole("textbox")).toHaveTextContent("Rascunho preservado");
    expect(await screen.findByRole("textbox")).toHaveAttribute("contenteditable", "true");
  });

  it("starts without demo messages and opens a real selected conversation", async () => {
    const { rerender } = render(<TestChat library={emptyLibrary()} />);
    expect(screen.getByText("Seu próximo projeto começa aqui")).toBeInTheDocument();
    expect(call).not.toHaveBeenCalled();
    rerender(<TestChat />);
    await screen.findByRole("heading", { name: "Primeira conversa" });
    expect(await screen.findByRole("textbox")).toHaveAttribute("contenteditable", "true");
    expect(screen.getByRole("button", { name: "Adicionar anexo" })).toBeEnabled();
    expect(call).toHaveBeenCalledWith("get_chat", { conversationId: "c1" });
  });
  it("uses the vertical brand and moves the same composer into the transcript after the first turn", async () => {
    render(<TestChat />);
    const stage = await screen.findByRole("region", { name: "Nova conversa" });
    const editor = within(stage).getByRole("textbox", { name: "Mensagem" });
    expect(within(stage).getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "/logo_vertical.png");
    expect(within(stage).getByRole("heading", { name: "O que vamos construir?" })).toBeVisible();
    expect(editor).toBeVisible();
    expect(stage).toHaveAttribute("data-empty", "true");
    expect(screen.queryByLabelText("Histórico de mensagens")).not.toBeInTheDocument();
    await update({ ...emptyChat(), revision: 2, turns: [savedTurn()] });
    await waitFor(() => expect(screen.queryByRole("region", { name: "Nova conversa" })).not.toBeInTheDocument());
    expect(stage).toHaveAttribute("data-empty", "false");
    expect(screen.getByRole("textbox", { name: "Mensagem" })).toBe(editor);
    expect(screen.queryByRole("heading", { name: "O que vamos construir?" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Histórico de mensagens")).toHaveTextContent("Leia o README");
  });
  it("restores messages, provider summary and real tool results", async () => {
    const user = userEvent.setup(); call.mockResolvedValue({ ...emptyChat(), turns: [savedTurn()] });
    render(<TestChat />);
    expect(await screen.findByText("Leia o README")).toBeInTheDocument();
    expect(await screen.findByText("Tauri")).toBeInTheDocument();
    expect(screen.queryByText("Gemini 2.5 Pro")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Trabalhou por/ }));
    await user.click(screen.getByRole("button", { name: /Verificando o projeto/ }));
    expect(screen.getByText("Verificando o projeto.", { selector: "p" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: /Leitura de arquivo/ }));
    expect(screen.getByText("# Jarvis")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Copiar" })).not.toBeInTheDocument();
  });
  it("shows elapsed chat time from the turn start while execution is active", async () => {
    const turn = {
      ...savedTurn(),
      createdAt: Date.now() - 11_000,
      durationMs: 1_000,
      status: "running" as const,
      steps: [{ durationMs: 1_000, summary: "Conferindo o contrato", text: "", tools: [], usage: null }],
    };
    call.mockResolvedValue({ ...emptyChat(), activeTurnId: turn.id, turns: [turn] });
    render(<TestChat />);
    expect(await screen.findByLabelText("Tempo total da execução")).toHaveTextContent(/1[1-3]s/);
  });
  it("restaura os registros de compactação na posição da conversa e não os duplica nos updates", async () => {
    const turn = savedTurn();
    const events = [
      { id: "auto", createdAt: turn.createdAt + 1000, turnId: turn.id, afterTurn: false, automatic: true, tokensBefore: 25000, tokensAfter: 2000 },
      { id: "manual", createdAt: turn.createdAt + 10000, turnId: turn.id, afterTurn: true, automatic: false, tokensBefore: 9000, tokensAfter: 1000 },
    ];
    const snapshot = { ...emptyChat(), turns: [turn], compactions: events };
    call.mockResolvedValue(snapshot);
    const first = render(<TestChat />);
    const manual = await screen.findByRole("note", { name: "Compactação manual concluída" });
    const automatic = screen.getByRole("note", { name: "Compactação automática concluída" });
    const assistant = screen.getByTestId("assistant-message-turn1");
    expect(automatic.compareDocumentPosition(assistant) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(assistant.compareDocumentPosition(manual) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await update({ ...snapshot, revision: 2 });
    expect(screen.getAllByRole("note")).toHaveLength(2);
    first.unmount();
    render(<TestChat />);
    expect(await screen.findByRole("note", { name: "Compactação manual concluída" })).toBeVisible();
    expect(screen.getAllByRole("note")).toHaveLength(2);
  });
  it("restaura skills explícitas como badges no histórico", async () => {
    call.mockResolvedValue({ ...emptyChat(), turns: [{ ...savedTurn(), user: "/review Confira o README", parts: [{ type: "skill", id: "review-id", name: "review" }, { type: "text", text: " Confira o README" }] }] });
    render(<TestChat />);
    const badge = await screen.findByTitle("Skill: review");
    expect(badge).toHaveTextContent("review");
    expect(within(screen.getByTestId("user-message-turn1")).getByText("Confira o README")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Remover skill review" })).not.toBeInTheDocument();
  });
  it("groups four tool steps under one compact summary while preserving nested details", async () => {
    const user = userEvent.setup();
    const turn = savedTurn();
    turn.durationMs = 11000;
    turn.steps = [0, 1, 2, 3].map(index => ({ ...turn.steps[0], summary: "", text: index === 0 ? "Vou conferir os arquivos." : "", tools: [{ ...turn.steps[0].tools[0], id: `read-${index}`, args: { path: `file-${index}.md` }, output: `Conteúdo ${index}` }] }));
    turn.steps.push({ durationMs: 1000, summary: "", text: "Este projeto organiza tarefas.", tools: [], usage: null });
    call.mockResolvedValue({ ...emptyChat(), turns: [turn] });
    render(<TestChat />);
    expect(await screen.findByText("Este projeto organiza tarefas.")).toBeInTheDocument();
    expect(screen.getAllByTestId(/^assistant-message-/)).toHaveLength(1);
    const summary = screen.getByRole("button", { name: /Trabalhou por 11s.*4 ações/ });
    expect(summary).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: /Leitura de arquivo/ })).not.toBeInTheDocument();
    summary.focus();
    await user.keyboard("{Enter}");
    expect(screen.getAllByRole("button", { name: /Leitura de arquivo/ })).toHaveLength(4);
    expect(screen.queryByText("Conteúdo 0")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Leitura de arquivo.*file-0/ }));
    expect(screen.getByText("Conteúdo 0")).toBeVisible();
    expect(screen.queryByText("Conteúdo 1")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Observações/ }));
    expect(screen.getByText("Vou conferir os arquivos.")).toBeVisible();
    await user.click(summary);
    expect(within(screen.getByLabelText("Processamento do Jarvis")).getAllByRole("button")).toHaveLength(1);
    expect(screen.getByText("Este projeto organiza tarefas.")).toBeVisible();
  });
  it("sends selected options, streams revisioned results and cancels only the current turn", async () => {
    const user = userEvent.setup();
    const running: ChatSnapshot = { ...emptyChat(), revision: 2, turns: [{ ...savedTurn(), status: "running", steps: [] }], activeTurnId: "turn1" };
    call.mockImplementation(async command => command === "start_agent_turn" ? running : emptyChat());
    render(<TestChat />);
    await screen.findByRole("textbox");
    await user.type(screen.getByRole("textbox"), "Leia o README");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(call).toHaveBeenCalledWith("start_agent_turn", { conversationId: "c1", content: "Leia o README", options: { account: "openai-codex-pessoal", model: "model", reasoning: "medium", mode: "build", workflow: "standard", approvalMode: "yolo" } });
    await screen.findByRole("button", { name: "Interromper execução" });
    expect(screen.getByRole("group", { name: "Mensagem e opções de envio" })).toHaveAttribute("data-working", "true");
    expect(screen.getByRole("main", { name: "Conversa" })).not.toHaveAttribute("data-working");
    await update({ ...running, revision: 4, turns: [{ ...savedTurn(), status: "running" }] });
    expect(await screen.findByText("Tauri")).toBeInTheDocument();
    await update({ ...running, revision: 3 });
    expect(screen.getByText("Tauri")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Interromper execução" }));
    expect(call).toHaveBeenCalledWith("cancel_agent_turn", { conversationId: "c1", turnId: "turn1" });
    await update({ ...emptyChat(), revision: 5, turns: [savedTurn()] });
    expect(screen.getByRole("group", { name: "Mensagem e opções de envio" })).not.toHaveAttribute("data-working");
  });
  it("shows exact tool arguments and correlates a manual denial", async () => {
    const user = userEvent.setup();
    const tool = { ...savedTurn().steps[0].tools[0], name: "edit", args: { path: "README.md", oldText: "before", newText: "after" }, status: "pending" as const };
    call.mockResolvedValue({ ...emptyChat(), activeTurnId: "turn1", turns: [{ ...savedTurn(), status: "running" }], pendingApproval: tool });
    render(<TestChat />);
    expect(await screen.findByText("before")).toBeInTheDocument(); expect(screen.getByText("after")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Recusar" }));
    expect(call).toHaveBeenCalledWith("approve_agent_tool", { conversationId: "c1", turnId: "turn1", toolId: "tool1", approved: false });
  });
  it("identifies a Beads mutation as a task change and asks for approval", async () => {
    const user = userEvent.setup();
    const tool = { ...savedTurn().steps[0].tools[0], name: "beads_update", args: { id: "project-task", title: "Novo título" }, status: "pending" as const };
    call.mockResolvedValue({ ...emptyChat(), activeTurnId: "turn1", turns: [{ ...savedTurn(), status: "running" }], pendingApproval: tool });
    render(<TestChat />);
    expect(await screen.findByText("Autorizar alteração de tarefa?")).toBeInTheDocument();
    expect(screen.getByText("Novo título")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Recusar" }));
    expect(call).toHaveBeenCalledWith("approve_agent_tool", { conversationId: "c1", turnId: "turn1", toolId: "tool1", approved: false });
  });
  it("opens the supervised authoring drawer and correlates approval to the active turn", async () => {
    const user = userEvent.setup();
    const proposal = {
      turnId: "turn1", toolId: "author-1", action: "create" as const, catalogRevision: 2,
      summary: "Criar um agente que revise acessibilidade.", agentReferences: [],
      target: { kind: "agent" as const, before: null, after: {
        id: "a".repeat(32), name: "Revisor de acessibilidade", description: "Revisa WCAG.",
        instructions: "## Objetivo\n\nRevisar a interface.", capability: "read_only" as const,
        deniedTools: [], model: null, appearance: { icon: "shield" as const, color: "cyan" as const },
      } },
    };
    const running = { ...emptyChat(), activeTurnId: "turn1", turns: [{ ...savedTurn(), status: "running" as const }], pendingAuthoring: proposal };
    call.mockImplementation(async command => command === "answer_agent_authoring" ? { ...running, revision: 3, pendingAuthoring: null } : running);
    render(<TestChat />);
    expect(await screen.findByRole("heading", { name: "Revisar alteração no Jarvis" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Aprovar e salvar" }));
    expect(call).toHaveBeenCalledWith("answer_agent_authoring", { conversationId: "c1", decision: { turnId: "turn1", toolId: "author-1", approved: true, note: null } });
    expect(await screen.findByRole("textbox", { name: "Mensagem" })).toBeVisible();
  });
  it("answers questions through native IPC while keeping the composer draft and queue independent", async () => {
    const user = userEvent.setup();
    const pendingQuestion = { turnId: "turn1", toolId: "ask1", questions: [{ id: "theme", question: "Qual tema prefere?", options: [{ label: "Escuro" }, { label: "Claro" }] }] };
    const tool = { id: "ask1", name: "ask_user", status: "running" as const, args: { questions: pendingQuestion.questions }, output: "", durationMs: 0 };
    const turn = { ...savedTurn(), status: "running" as const, steps: [{ durationMs: 0, text: "", summary: "", usage: null, tools: [tool] }] };
    const response = { cancelled: false, answers: [{ id: "theme", value: "Escuro", selectedLabel: "Escuro" }] };
    const queue = [{ id: "q1", content: "Depois revise os testes", options: turn.options }];
    const running = { ...emptyChat(), activeTurnId: turn.id, turns: [turn], pendingQuestion, queuedMessages: queue };
    call.mockImplementation(async command => command === "answer_agent_question" ? {
      ...running, pendingQuestion: null, revision: 3,
      turns: [{ ...turn, steps: [{ ...turn.steps[0], tools: [{ ...tool, status: "completed", output: JSON.stringify(response) }] }] }],
    } : running);
    render(<TestChat />);
    const card = await screen.findByRole("region", { name: "Perguntas do Jarvis" });
    const composer = await screen.findByRole("textbox", { name: "Mensagem" });
    await user.type(composer, "Meu rascunho");
    expect(screen.getByRole("region", { name: "Mensagens agendadas" })).toHaveTextContent(queue[0].content);
    expect(screen.queryByRole("region", { name: "Autorização de ferramenta" })).not.toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Escuro" }));
    await user.click(within(card).getByRole("button", { name: "Enviar respostas" }));
    expect(call).toHaveBeenCalledWith("answer_agent_question", { conversationId: "c1", turnId: "turn1", toolId: "ask1", response });
    expect(screen.queryByRole("region", { name: "Perguntas do Jarvis" })).not.toBeInTheDocument();
    expect(composer).toHaveTextContent("Meu rascunho");
    expect(composer).toHaveFocus();
    const history = screen.getByRole("button", { name: "Feita 1 pergunta" });
    expect(history).toHaveAttribute("aria-expanded", "false");
    await user.click(history);
    expect(screen.getByText("Qual tema prefere?")).toBeVisible();
    expect(screen.getByText("Escuro")).toBeVisible();
    await update({ ...running, revision: 2 });
    expect(screen.queryByRole("region", { name: "Perguntas do Jarvis" })).not.toBeInTheDocument();
  });
  it("restores partial question answers after switching conversations and ignores late answer snapshots", async () => {
    const user = userEvent.setup();
    const pendingQuestion = { turnId: "turn1", toolId: "ask1", questions: [{ id: "theme", question: "Qual tema prefere?", options: [] }] };
    const running = { ...emptyChat(), activeTurnId: "turn1", pendingQuestion };
    let resolve!: (value: unknown) => void;
    call.mockImplementation(async (command, args) => {
      if (command === "answer_agent_question") return new Promise(done => { resolve = done; });
      return typeof args === "object" && args !== null && "conversationId" in args && args.conversationId === "c2" ? emptyChat("c2") : running;
    });
    const { rerender } = render(<TestChat />);
    await user.type(await screen.findByRole("textbox", { name: "Sua resposta" }), "Um tema escuro");
    const next = populatedLibrary(); next.selection = { workspaceId: "w2", projectId: "p2", conversationId: "c2" };
    rerender(<TestChat library={next} />);
    await screen.findByRole("heading", { name: "Conversa do trabalho" });
    expect(screen.queryByRole("region", { name: "Perguntas do Jarvis" })).not.toBeInTheDocument();
    rerender(<TestChat />);
    expect(await screen.findByRole("textbox", { name: "Sua resposta" })).toHaveValue("Um tema escuro");
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    rerender(<TestChat library={next} />);
    await screen.findByRole("heading", { name: "Conversa do trabalho" });
    await act(async () => resolve({ ...running, revision: 10, pendingQuestion: null }));
    expect(screen.getByRole("heading", { name: "Conversa do trabalho" })).toBeVisible();
    expect(screen.queryByRole("region", { name: "Perguntas do Jarvis" })).not.toBeInTheDocument();
  });
  it("reports missing history and retries without inventing a session", async () => {
    const user = userEvent.setup(); call.mockRejectedValueOnce({ message: "Histórico ausente." });
    render(<TestChat />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Histórico ausente.");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    await screen.findByRole("textbox");
  });
  it("returns from the browser to Chat when the agent needs approval", async () => {
    let activeId: string | null = "browser-1";
    call.mockImplementation(async (command, args) => {
      if (command === "get_browser_tabs") return { tabs: [{ id: "browser-1", conversationId: emptyChat().conversationId, title: "Aplicativo local", url: "http://localhost:5173/", loading: false }], activeId };
      if (command === "browser_command" && (args as { request: { action: string } }).request.action === "select") { activeId = null; return {}; }
      return emptyChat();
    });
    render(<TestChat />);
    await screen.findByRole("textbox", { name: "Endereço do navegador" });
    await update({ ...emptyChat(), revision: 2, pendingApproval: { id: "browser-click", name: "browser_click", args: { id: "browser-1", element: "element-1" }, status: "pending", output: "", durationMs: 0 } });
    expect(await screen.findByRole("button", { name: "Autorizar uma vez" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("Autorizar ação no navegador?")).toBeVisible();
  });
  it("keeps a rejected message draft and disables send without an account", async () => {
    const user = userEvent.setup();
    call.mockImplementation(async command => { if (command === "start_agent_turn") throw { message:"Não salvo" }; return emptyChat(); });
    const { rerender } = render(<TestChat />); await screen.findByRole("textbox");
    await user.type(screen.getByRole("textbox"), "Meu pedido"); await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveAttribute("contenteditable", "true"));
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveTextContent("Meu pedido"));
    rerender(<TestChat connected={false} />); expect(screen.getByRole("textbox")).toHaveAttribute("contenteditable", "true");
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  });
  it("ignores old conversation loads and unrelated events after switching", async () => {
    let resolve!: (value: unknown) => void;
    call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    const { rerender } = render(<TestChat />);
    await waitFor(() => expect(call).toHaveBeenCalledTimes(1));
    const next = populatedLibrary(); next.selection = { workspaceId:"w2",projectId:"p2",conversationId:"c2" };
    call.mockResolvedValue(emptyChat("c2")); rerender(<TestChat library={next} />);
    await screen.findByRole("heading", { name:"Conversa do trabalho" });
    await act(async () => resolve({ ...emptyChat(), turns:[savedTurn()] }));
    await update({ ...emptyChat(), revision:99,turns:[savedTurn()] });
    expect(screen.queryByText("Leia o README")).not.toBeInTheDocument();
    rerender(<TestChat library={emptyLibrary()} />); expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });
  it("updates renamed context without reopening the history", async () => {
    const { rerender } = render(<TestChat />); await screen.findByRole("textbox");
    const library = populatedLibrary(); library.projects[0].name = "Meu projeto"; library.conversations[0].title = "Título editado";
    rerender(<TestChat library={library} />);
    expect(screen.getByRole("heading", { name:"Título editado" })).toBeInTheDocument(); expect(screen.getByTitle("/projects/jarvis")).toHaveTextContent("Pessoal / Meu projeto"); expect(call.mock.calls.filter(([command]) => command === "get_chat")).toHaveLength(1);
  });
});
