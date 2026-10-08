import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { companionChatSchema, type CompanionChat } from "@/core/companion";
import type { PendingAuthoring } from "@/core/authoring";
import { CompanionChatPane } from "./CompanionChatPane";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@/components/chat/LazyChatMarkdown", () => ({ LazyChatMarkdown: ({ content }: { content: string }) => <p data-testid="project-markdown">{content}</p> }));
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
const mcpProposal = (): PendingAuthoring => ({ turnId: "turn-1", toolId: "mcp-add", action: "create", summary: "Adicionar Monday ao Jarvis", catalogRevision: null, agentReferences: [], target: { kind: "mcp", server: {
  name: "Monday", transport: "http", command: null, args: [], url: "https://mcp.example.com/monday", cwd: null, enabled: true, envKeys: [], headerKeys: ["Authorization"],
} } });

describe("Jarvito chat", () => {
  it("reviews a global hook directly in the island without requesting project scope", async () => {
    const user = userEvent.setup(); const onQuestionChange = vi.fn();
    const original = call.getMockImplementation()!;
    call.mockImplementation(async (command, args, invokeOptions) => command === "answer_agent_authoring"
      ? { ...globalChat.chat, revision: 3, pendingAuthoring: null }
      : original(command, args, invokeOptions));
    globalChat.chat.activeTurnId = "turn-1";
    globalChat.chat.pendingAuthoring = { turnId: "turn-1", toolId: "hook-add", action: "create", catalogRevision: 0, summary: "Adicionar diagnóstico dos comandos", agentReferences: [], target: { kind: "hook", before: null, after: {
      id: "a".repeat(32), name: "Diagnóstico", event: "PreToolUse", command: "node hooks/check.js", matcher: "bash", timeoutSeconds: 30, enabled: true,
    } } };
    render(<CompanionChatPane externalQuestions onQuestionChange={onQuestionChange} />);
    expect(await screen.findByText("node hooks/check.js")).toBeVisible();
    expect(onQuestionChange).toHaveBeenLastCalledWith(null);
    expect(screen.queryByRole("button", { name: "Continuar no Jarvis" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Aprovar e salvar" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("answer_agent_authoring", { conversationId: "global-chat", decision: { turnId: "turn-1", toolId: "hook-add", approved: true, note: null } }));
    expect(call.mock.calls.some(([command]) => command === "companion_open_conversation")).toBe(false);
  });
  it("persists its model choice only for the selected conversation", async () => {
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.click(screen.getByRole("button", { name: "Modelo do Jarvito" }));
    (await screen.findByRole("menuitem", { name: "codex" })).focus();
    await user.keyboard("{ArrowRight}");
    (await screen.findByRole("menuitem", { name: "GPT-6" })).focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_chat_agent_model", {
      conversationId: "global-chat", key: "standard/builder", choice: { executor: "jarvis", account: "codex", model: "gpt-6", reasoning: "high" },
    }));
    expect(call).not.toHaveBeenCalledWith("set_agent_model", expect.anything());
  });

  it("highlights a missing saved model and blocks sending without discarding the draft", async () => {
    globalChat.options = { ...options, account: "removed", model: "missing" };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Continue{Enter}");
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Continue");
    expect(call).not.toHaveBeenCalledWith("send_companion_message", expect.anything());
  });

  it("retains the available saved secondary when changing the conversation primary model", async () => {
    const fallback = { executor: "jarvis" as const, account: "codex", model: "backup", reasoning: "high" };
    globalChat.options = { ...options, modelSelection: { executor: "jarvis", account: "codex", model: "gpt-6", reasoning: "high", fallback } };
    const original = call.getMockImplementation()!;
    call.mockImplementation(async (command, args, invokeOptions) => command === "get_companion_models" ? [{ provider: "codex", providerKind: "openai-codex", models: [
      { value: "codex/gpt-6", label: "GPT-6", reasoningLevels: ["high"], defaultReasoningLevel: "high" },
      { value: "codex/backup", label: "Backup", reasoningLevels: ["high"], defaultReasoningLevel: "high" },
      { value: "codex/gpt-6-sol", label: "GPT-6 Sol", reasoningLevels: ["high"], defaultReasoningLevel: "high" },
    ] }] : original(command, args, invokeOptions));
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).not.toHaveAttribute("aria-invalid");
    await user.click(screen.getByRole("button", { name: "Modelo do Jarvito" }));
    (await screen.findByRole("menuitem", { name: "codex" })).focus();
    await user.keyboard("{ArrowRight}");
    (await screen.findByRole("menuitem", { name: "GPT-6 Sol" })).focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_chat_agent_model", {
      conversationId: "global-chat", key: "standard/builder", choice: { executor: "jarvis", account: "codex", model: "gpt-6-sol", reasoning: "high", fallback },
    }));
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("GPT-6 Sol");
    expect(call).not.toHaveBeenCalledWith("set_agent_model", expect.anything());
  });

  it("blocks a missing saved secondary and clears it only after an explicit valid chat selection", async () => {
    const fallback = { executor: "jarvis" as const, account: "removed", model: "missing", reasoning: "high" };
    globalChat.options = { ...options, modelSelection: { executor: "jarvis", account: "codex", model: "gpt-6", reasoning: "high", fallback } };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    const picker = screen.getByRole("button", { name: "Modelo do Jarvito" });
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Continue{Enter}");
    expect(picker).toHaveTextContent("GPT-6");
    expect(picker).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Continue");
    expect(call).not.toHaveBeenCalledWith("send_companion_message", expect.anything());
    expect(call).not.toHaveBeenCalledWith("set_chat_agent_model", expect.anything());
    await user.click(picker);
    (await screen.findByRole("menuitem", { name: "codex" })).focus();
    await user.keyboard("{ArrowRight}");
    (await screen.findByRole("menuitem", { name: /^GPT-6/ })).focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_chat_agent_model", {
      conversationId: "global-chat", key: "standard/builder", choice: { executor: "jarvis", account: "codex", model: "gpt-6", reasoning: "high", fallback: null },
    }));
    expect(picker).not.toHaveAttribute("aria-invalid");
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Continue");
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(call).toHaveBeenCalledWith("send_companion_message", expect.objectContaining({ content: "Continue" }));
    expect(call).not.toHaveBeenCalledWith("set_agent_model", expect.anything());
  });

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
      if (command === "get_companion_models") return [{ provider: "codex", providerKind: "openai-codex", models: [{ value: "codex/gpt-6", label: "GPT-6", reasoningLevels: ["high"], defaultReasoningLevel: "high" }] }];
      if (command === "send_companion_message") return globalChat;
      if (command === "clear_companion_chat") {
        const revision = globalChat.chat.revision + 1;
        const previousOptions = globalChat.options;
        globalChat = { ...makeChat(), options: previousOptions };
        globalChat.chat.revision = revision;
        return globalChat;
      }
      if (command === "confirm_companion_project") return (args as { confirmed: boolean }).confirmed ? projectChat : { ...globalChat, proposal: null };
      return true;
    });
  });
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it.each([false, true])("approves a global MCP directly in Jarvito with externalQuestions=%s without opening a project conversation", async externalQuestions => {
    const user = userEvent.setup(); const onQuestionChange = vi.fn();
    const original = call.getMockImplementation()!;
    call.mockImplementation(async (command, args, invokeOptions) => {
      if (command === "answer_agent_authoring") {
        globalChat = { ...globalChat, chat: { ...globalChat.chat, revision: 3, pendingAuthoring: null } };
        return globalChat.chat;
      }
      return original(command, args, invokeOptions);
    });
    render(<CompanionChatPane externalQuestions={externalQuestions} onQuestionChange={onQuestionChange} />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Orientação preservada");
    const request = mcpProposal(); const serialized = JSON.stringify(request);
    globalChat = { ...globalChat, chat: { ...globalChat.chat, revision: 2, activeTurnId: "turn-1", pendingAuthoring: request } };
    act(() => events.get("companion:chat_changed")?.({ conversationId: "global-chat" }));
    expect(await screen.findByRole("dialog", { name: "Revisar servidor MCP" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Continuar no Jarvis" })).not.toBeInTheDocument();
    expect(onQuestionChange).toHaveBeenLastCalledWith(null);
    expect(screen.getByText("https://mcp.example.com/monday")).toBeVisible();
    expect(screen.getByRole("button", { name: "Aprovar e adicionar" })).toBeDisabled();
    const password = screen.getByLabelText("Cabeçalho Authorization");
    expect(password).toHaveAttribute("type", "password");
    await user.type(password, "Bearer private-jarvito-test");
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Revisar servidor MCP" })).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("answer_agent_authoring", expect.anything());
    expect(JSON.stringify(request)).toBe(serialized);
    await user.click(screen.getByRole("button", { name: "Aprovar e adicionar" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Revisar servidor MCP" })).not.toBeInTheDocument());
    expect(call).toHaveBeenCalledWith("answer_agent_authoring", { conversationId: "global-chat", decision: {
      turnId: "turn-1", toolId: "mcp-add", approved: true, note: null,
      mcpValues: { environment: {}, headers: { Authorization: "Bearer private-jarvito-test" } },
    } });
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Orientação preservada");
    expect(JSON.stringify(globalChat)).not.toContain("private-jarvito-test");
    expect(call.mock.calls.some(([command]) => command === "companion_open_conversation")).toBe(false);
  });

  it("keeps a global MCP pending when the island closes and explicitly refuses it without forwarding entered credentials", async () => {
    const user = userEvent.setup();
    globalChat.chat.activeTurnId = "turn-1"; globalChat.chat.pendingAuthoring = mcpProposal();
    const request = JSON.stringify(globalChat.chat.pendingAuthoring);
    const original = call.getMockImplementation()!;
    call.mockImplementation(async (command, args, invokeOptions) => command === "answer_agent_authoring"
      ? { ...globalChat.chat, revision: 2, pendingAuthoring: null }
      : original(command, args, invokeOptions));
    const view = render(<CompanionChatPane active externalQuestions />);
    await screen.findByRole("dialog", { name: "Revisar servidor MCP" });
    await user.type(screen.getByLabelText("Cabeçalho Authorization"), "do-not-retain");
    view.rerender(<CompanionChatPane active={false} externalQuestions />);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(JSON.stringify(globalChat.chat.pendingAuthoring)).toBe(request);
    expect(call).not.toHaveBeenCalledWith("answer_agent_authoring", expect.anything());
    view.rerender(<CompanionChatPane active externalQuestions />);
    await screen.findByRole("dialog", { name: "Revisar servidor MCP" });
    expect(screen.getByLabelText("Cabeçalho Authorization")).toHaveValue("");
    await user.type(screen.getByLabelText("Cabeçalho Authorization"), "never-send-on-refusal");
    await user.click(screen.getByRole("button", { name: "Recusar" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("answer_agent_authoring", { conversationId: "global-chat", decision: {
      turnId: "turn-1", toolId: "mcp-add", approved: false, note: null,
    } }));
  });

  it("keeps project MCP approval in the existing full-conversation route", async () => {
    const user = userEvent.setup();
    projectChat.chat.activeTurnId = "turn-1"; projectChat.chat.pendingAuthoring = mcpProposal();
    render(<CompanionChatPane />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.click(screen.getByRole("combobox", { name: "Conversa do Jarvito" }));
    await user.click(await screen.findByRole("option", { name: /Ajustar relatório/ }));
    await user.click(await screen.findByRole("button", { name: "Continuar no Jarvis" }));
    expect(call).toHaveBeenCalledWith("companion_open_conversation", { conversationId: "project-chat" });
    expect(screen.queryByRole("dialog", { name: "Revisar servidor MCP" })).not.toBeInTheDocument();
    expect(call).not.toHaveBeenCalledWith("answer_agent_authoring", expect.anything());
  });

  it("opens a passive global chat without a project or opening the main app", async () => {
    render(<CompanionChatPane />);
    expect(await screen.findByText("Oi, eu sou o Jarvito.")).toBeVisible();
    expect(screen.getByRole("combobox", { name: "Conversa do Jarvito" })).toHaveTextContent("Conversar com Jarvito");
    expect(screen.queryByText(/Ajuda sem projeto|Para trabalhar em arquivos, Jarvito pede sua confirmação/)).not.toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("get_companion_chat", undefined);
    expect(call.mock.calls.some(([name]) => name === "send_companion_message" || name === "companion_open_conversation")).toBe(false);
  });

  it.each(["conversation", "model"] as const)("closes the %s menu when hidden without losing the chat draft", async menu => {
    const user = userEvent.setup();
    const { rerender } = render(<CompanionChatPane active />);
    await screen.findByText("Oi, eu sou o Jarvito.");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Minha orientação");
    await user.click(menu === "conversation"
      ? screen.getByRole("combobox", { name: "Conversa do Jarvito" })
      : screen.getByRole("button", { name: "Modelo do Jarvito" }));
    const menuIsOpen = () => screen.queryByRole(menu === "conversation" ? "listbox" : "menu");
    await waitFor(() => expect(menuIsOpen()).toBeInTheDocument());
    rerender(<CompanionChatPane active={false} />);
    await waitFor(() => expect(menuIsOpen()).not.toBeInTheDocument());
    rerender(<CompanionChatPane active />);
    expect(menuIsOpen()).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Minha orientação");
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

  it("keeps the general assistant in plain text and does not expose coding progress", async () => {
    globalChat.chat.turns = [turn("Oi", "Olá, estou aqui.", "running")];
    globalChat.chat.activeTurnId = "turn-1";
    render(<CompanionChatPane />);
    expect(await screen.findByText("Olá, estou aqui.")).toBeVisible();
    expect(screen.queryByTestId("project-markdown")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Jarvito está pensando…");
    expect(screen.queryByText("Conferindo o pedido")).not.toBeInTheDocument();
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

  it("clears general messages, proposals and its draft while keeping the selected model", async () => {
    globalChat.options = options;
    globalChat.chat.turns = [turn("Minha pergunta antiga", "Minha resposta antiga")];
    globalChat.proposal = { id: "proposal-1", projectId: "project-1", projectName: "Portal", workspaceName: "Trabalho", conversationId: null, reason: "O pedido altera o projeto.", message: "Trabalhar no Portal." };
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Minha resposta antiga");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Rascunho antigo");
    await user.click(screen.getByRole("button", { name: "Limpar conversa" }));
    expect(call).toHaveBeenCalledWith("clear_companion_chat");
    expect(await screen.findByText("Oi, eu sou o Jarvito.")).toBeVisible();
    expect(screen.queryByText("Minha pergunta antiga")).not.toBeInTheDocument();
    expect(screen.queryByText("Minha resposta antiga")).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Continuar em um projeto" })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("");
    expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("codex · GPT-6");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Novo assunto");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(call).toHaveBeenCalledWith("send_companion_message", { conversationId: null, content: "Novo assunto", options: { ...options, executor: "jarvis" } });
  });

  it("preserves the general history and draft if clearing fails and allows retrying", async () => {
    globalChat.chat.turns = [turn("Minha pergunta", "Minha resposta")];
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Minha resposta");
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Ainda não enviei");
    call.mockRejectedValueOnce("Não foi possível limpar a conversa.");
    await user.click(screen.getByRole("button", { name: "Limpar conversa" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível limpar");
    expect(screen.getByText("Minha resposta")).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toHaveValue("Ainda não enviei");
    expect(screen.getByRole("button", { name: "Limpar conversa" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Limpar conversa" }));
    expect(await screen.findByText("Oi, eu sou o Jarvito.")).toBeVisible();
  });

  it("keeps a new draft typed while the general conversation is being cleared", async () => {
    globalChat.chat.turns = [turn("Pergunta antiga", "Resposta antiga")];
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Resposta antiga");
    const input = screen.getByRole("textbox", { name: "Mensagem para Jarvito" });
    await user.type(input, "Rascunho antigo");
    let finish: ((value: CompanionChat) => void) | undefined;
    call.mockImplementationOnce(() => new Promise<CompanionChat>(resolve => { finish = resolve; }));
    await user.click(screen.getByRole("button", { name: "Limpar conversa" }));
    await user.clear(input); await user.type(input, "Meu próximo pedido");
    globalChat = makeChat(); globalChat.chat.revision = 2;
    await act(async () => { finish?.(globalChat); });
    expect(await screen.findByText("Oi, eu sou o Jarvito.")).toBeVisible();
    expect(input).toHaveValue("Meu próximo pedido");
    await user.click(screen.getByRole("combobox", { name: "Conversa do Jarvito" }));
    await user.click(await screen.findByRole("option", { name: /Ajustar relatório.*Trabalho.*Portal/ }));
    await screen.findByText("Projeto Portal · conversa compartilhada com o Jarvis");
    await user.click(screen.getByRole("combobox", { name: "Conversa do Jarvito" }));
    await user.click(await screen.findByRole("option", { name: /Conversar com Jarvito.*Sem projeto/ }));
    await screen.findByText("Oi, eu sou o Jarvito.");
    expect(input).toHaveValue("Meu próximo pedido");
  });

  it.each(["running", "queued", "compacting"])("prevents clearing a general conversation with ongoing work: %s", async status => {
    globalChat.chat.turns = [turn("Meu pedido", "Meu histórico", status === "running" ? "running" : "completed")];
    if (status === "running") globalChat.chat.activeTurnId = "turn-1";
    if (status === "queued") globalChat.chat.queuedMessages = [{ id: "queued-1", content: "Pedido na fila", options }];
    if (status === "compacting") globalChat.chat.compacting = true;
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Meu histórico");
    const clear = screen.getByRole("button", { name: "Limpar conversa" });
    expect(clear).toBeDisabled();
    await user.click(clear);
    expect(call.mock.calls.some(([command]) => command === "clear_companion_chat")).toBe(false);
  });

  it("does not restore cleared messages when an earlier refresh resolves later", async () => {
    globalChat.chat.turns = [turn("Minha pergunta antiga", "Minha resposta antiga")];
    const previous = companionChatSchema.parse(globalChat);
    const user = userEvent.setup(); render(<CompanionChatPane />);
    await screen.findByText("Minha resposta antiga");
    let finish: ((value: CompanionChat) => void) | undefined;
    call.mockImplementationOnce(() => new Promise<CompanionChat>(resolve => { finish = resolve; }));
    act(() => events.get("companion:chat_changed")?.({ conversationId: globalChat.conversationId }));
    await waitFor(() => expect(finish).toBeDefined());
    await user.click(screen.getByRole("button", { name: "Limpar conversa" }));
    await screen.findByText("Oi, eu sou o Jarvito.");
    await act(async () => { finish?.(previous); });
    expect(screen.getByText("Oi, eu sou o Jarvito.")).toBeVisible();
    expect(screen.queryByText("Minha resposta antiga")).not.toBeInTheDocument();
  });

  it("keeps composer and model controls outside the independently scrolling transcript", async () => {
    const user = userEvent.setup();
    render(<div style={{ height: 138 }}><CompanionChatPane /></div>);
    await screen.findByText("Oi, eu sou o Jarvito.");
    const surface = screen.getByRole("region", { name: "Conversa e controles do Jarvito" });
    const viewport = surface.querySelector<HTMLElement>("[data-slot=scroll-area-viewport]");
    if (!viewport) throw new Error("Missing chat scroll viewport");
    expect(viewport).not.toContainElement(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }));
    expect(viewport).not.toContainElement(screen.getByRole("button", { name: "Modelo do Jarvito" }));
    expect(surface).toContainElement(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }));
    expect(surface).toContainElement(screen.getByRole("button", { name: "Modelo do Jarvito" }));
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
    expect(screen.queryByRole("button", { name: "Limpar conversa" })).not.toBeInTheDocument();
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

  it.each([ ["flow", "Planejamento", "Fluxo"], ["agent", "Construtor", "Agente"] ] as const)("shows the chosen %s before confirming the project handoff", async (kind, name, label) => {
    globalChat.proposal = companionChatSchema.parse({ ...globalChat, proposal: {
      id: "proposal-1", projectId: "project-1", projectName: "Portal", workspaceName: "Trabalho", conversationId: null,
      reason: "Execução solicitada na conversa.", message: "Implementar a busca.", execution: { kind, id: "selected-executor", name },
    } }).proposal;
    const user = userEvent.setup(); render(<CompanionChatPane />);
    expect(await screen.findByText(`${label}: ${name}`)).toBeVisible();
    expect(call.mock.calls.some(([command]) => command === "confirm_companion_project")).toBe(false);
    await user.click(screen.getByRole("button", { name: "Confirmar" }));
    expect(call).toHaveBeenCalledWith("confirm_companion_project", { proposalId: "proposal-1", confirmed: true });
    expect(await screen.findByText("Projeto Portal · conversa compartilhada com o Jarvis")).toBeVisible();
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
    (await screen.findByRole("menuitem", { name: "GPT-6" })).focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("GPT-6"));
    await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Continue o pedido");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(call).toHaveBeenCalledWith("send_companion_message", { conversationId: null, content: "Continue o pedido", options: { ...options, executor: "jarvis", model: "gpt-6", reasoning: "high" } });
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

it("persists Fast in Jarvito's chat and removes the prior tier when returning to Normal", async () => {
  globalChat.options = { ...options, serviceTier: "priority" };
  const original = call.getMockImplementation()!;
  call.mockImplementation(async (command, args, invokeOptions) => command === "get_companion_models" ? [{ provider: "codex", providerKind: "openai-codex", models: [{ value: "codex/gpt-6", label: "GPT-6", reasoningLevels: ["high"], defaultReasoningLevel: "high", supportsFast: true }] }] : original(command, args, invokeOptions));
  const user = userEvent.setup(); render(<CompanionChatPane />);
  await screen.findByText("Oi, eu sou o Jarvito.");
  await waitFor(() => expect(screen.getByRole("button", { name: "Modelo do Jarvito" })).toHaveTextContent("Fast"));
  screen.getByRole("button", { name: "Modelo do Jarvito" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: /Velocidade/ })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitemradio", { name: "Normal" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("set_chat_agent_model", { conversationId: "global-chat", key: "standard/builder", choice: { executor: "jarvis", account: "codex", model: "gpt-6", reasoning: "high" } }));
  await user.type(screen.getByRole("textbox", { name: "Mensagem para Jarvito" }), "Continue normal{Enter}");
  await waitFor(() => expect(call.mock.calls.some(([command]) => command === "send_companion_message")).toBe(true));
  const sent = call.mock.calls.find(([command]) => command === "send_companion_message");
  expect(sent?.[1]).toMatchObject({ content: "Continue normal", options: { account: "codex", model: "gpt-6" } });
  expect(sent?.[1]).toHaveProperty("options");
  expect((sent?.[1] as { options: object }).options).not.toHaveProperty("serviceTier");
});
