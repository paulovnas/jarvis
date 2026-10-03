import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { populatedLibrary } from "@/test/library-fixtures";
import type { WorkflowAgent } from "@/core/workflow";
import { customAgent, customCatalog } from "@/test/workflow-fixtures";
import { remoteTurnOptions } from "./remote-choices";
import type { RemoteChoices } from "./client";
import { RemoteClient, RemoteError, type RemoteChat, type RemoteLibrary } from "./client";
import { RemoteApp } from "./RemoteApp";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers(); });
const bundle = (id = "c1"): RemoteChat => ({ chat: { ...emptyChat(id), turns: [{ ...savedTurn(), user: `Mensagem ${id}` }] }, workflow: null, options: null });
const chatChoices = (): RemoteChoices => ({ catalog: customCatalog, defaults: {}, overrides: {}, models: [{ provider: "openai-codex-pessoal", providerKind: "openai-codex", models: [{ value: "openai-codex-pessoal/model", label: "Modelo atual", reasoningLevels: ["medium"], defaultReasoningLevel: "medium" }, { value: "openai-codex-pessoal/sol", label: "GPT Sol", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" }] }] });
function fixtures() {
  const client = new RemoteClient(); const library: RemoteLibrary = { library: populatedLibrary(), runtime: [] };
  vi.spyOn(client, "session").mockResolvedValue({ deviceId: "phone", name: "Celular" });
  const list = vi.spyOn(client, "library").mockResolvedValue(library);
  const chat = vi.spyOn(client, "chat").mockImplementation(async id => bundle(id));
  const beads = vi.spyOn(client, "beads").mockResolvedValue({ issues: [] });
  const choices = vi.spyOn(client, "choices").mockResolvedValue(chatChoices());
  const usage = vi.spyOn(client, "usage").mockResolvedValue([]);
  const setModel = vi.spyOn(client, "setChatModel").mockImplementation(async (_id, key, choice) => ({ [key]: choice }));
  const mutate = vi.spyOn(client, "mutate").mockResolvedValue({ ok: true });
  return { client, library, list, chat, beads, choices, usage, setModel, mutate };
}
async function openFirst(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Pessoal" }));
  await user.click(screen.getByRole("button", { name: "Jarvis" }));
  await user.click(screen.getByRole("button", { name: "Primeira conversa" }));
  await screen.findByRole("textbox", { name: "Mensagem" });
}

it("opens provider limits from the mobile header without opening a chat", async () => {
  const user = userEvent.setup(); const { client, usage, chat } = fixtures();
  render(<RemoteApp client={client} />);
  await user.click(await screen.findByRole("button", { name: "Limites dos provedores" }));
  expect(await screen.findByRole("dialog", { name: "Limites dos provedores" })).toBeVisible();
  await waitFor(() => expect(usage).toHaveBeenCalledExactlyOnceWith(true));
  expect(chat).not.toHaveBeenCalled();
  expect(await screen.findByText("Nenhum limite disponível")).toBeVisible();
});

it("selects an agent and starts the first message entirely from the phone", async () => {
  const user = userEvent.setup(); const { client, chat, choices, mutate } = fixtures();
  chat.mockResolvedValue({ ...bundle(), chat: emptyChat() });
  const data = chatChoices();
  data.defaults["standard/builder"] = { account: "openai-codex-pessoal", model: "sol", reasoning: "high" };
  data.catalog = { ...customCatalog, agents: [{ ...customAgent, model: data.defaults["standard/builder"] }] };
  choices.mockResolvedValue(data);
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.click(await screen.findByRole("button", { name: "Selecionar fluxo ou agente" }));
  await user.click(await screen.findByRole("option", { name: customAgent.name }));
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Investigue esse projeto");
  await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
  expect(mutate).toHaveBeenCalledExactlyOnceWith("message", { conversationId: "c1", content: "Investigue esse projeto", options: { account: "openai-codex-pessoal", model: "sol", reasoning: "high", executor: undefined, workflow: "custom", customWorkflowId: null, customAgentId: customAgent.id, mode: "build", approvalMode: "yolo" } });
});

it("saves the next-message model in this chat while preserving current work", async () => {
  const user = userEvent.setup(); const { client, chat, choices, setModel, mutate } = fixtures();
  const running = bundle(); running.chat.activeTurnId = "turn1";
  running.chat.turns[0].status = "running";
  chat.mockResolvedValue(running);
  const data = chatChoices(); choices.mockResolvedValue(data);
  setModel.mockImplementation(async (_id, key, choice) => { data.overrides = { ...data.overrides, [key]: choice }; return data.overrides; });
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Depois revise isso");
  expect(screen.getByRole("button", { name: "Selecionar fluxo ou agente" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("option", { name: "openai-codex-pessoal · GPT Sol" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  const choice = { executor: "jarvis" as const, account: "openai-codex-pessoal", model: "sol", reasoning: "high" };
  expect(setModel).toHaveBeenCalledExactlyOnceWith("c1", "standard/builder", choice);
  expect(running.chat.turns[0].options.model).toBe("model");
  expect(running.chat.activeTurnId).toBe("turn1");
  await user.click(screen.getByRole("button", { name: "Enviar mensagem para a fila" }));
  expect(mutate).toHaveBeenCalledExactlyOnceWith("message", { conversationId: "c1", content: "Depois revise isso", options: remoteTurnOptions("standard", choice, running.chat.turns[0].options) });
});

it("blocks a removed model while preserving the draft until a valid choice is saved", async () => {
  const user = userEvent.setup(); const { client, choices, setModel, mutate } = fixtures();
  const data = chatChoices(); data.overrides["standard/builder"] = { account: "retired", model: "missing", reasoning: null };
  choices.mockResolvedValue(data);
  setModel.mockImplementation(async (_id, key, choice) => { data.overrides = { ...data.overrides, [key]: choice }; return data.overrides; });
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Meu texto preservado");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveAttribute("aria-invalid", "true");
  await user.click(trigger);
  await user.click(await screen.findByRole("option", { name: "openai-codex-pessoal · GPT Sol" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue("Meu texto preservado");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeEnabled();
  expect(mutate).not.toHaveBeenCalled();
});

it("keeps agent choices scoped to the phone conversation and ignores late model reads", async () => {
  const user = userEvent.setup(); const { client, choices, setModel } = fixtures();
  const first = chatChoices(); const second = chatChoices();
  let release: ((data: RemoteChoices) => void) | undefined;
  choices.mockResolvedValue(first);
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.click(await screen.findByRole("button", { name: "Selecionar fluxo ou agente" }));
  await user.click(await screen.findByRole("option", { name: customAgent.name }));
  const stale = chatChoices();
  choices.mockImplementationOnce(() => new Promise(resolve => { release = resolve; })).mockResolvedValue(first);
  act(() => { window.dispatchEvent(new Event("focus")); });
  await waitFor(() => expect(release).toBeDefined());
  setModel.mockImplementation(async (_id, key, choice) => { first.overrides = { ...first.overrides, [key]: choice }; return first.overrides; });
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("option", { name: "openai-codex-pessoal · GPT Sol" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await act(async () => { release?.(stale); });
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("GPT Sol");
  expect(setModel).toHaveBeenCalledTimes(1);

  choices.mockImplementation(async id => id === "c1" ? first : second);
  for (let index = 0; index < 3; index++) await user.click(screen.getByRole("button", { name: "Voltar" }));
  await user.click(screen.getByRole("button", { name: "Trabalho" })); await user.click(screen.getByRole("button", { name: "Outro projeto" })); await user.click(screen.getByRole("button", { name: "Conversa do trabalho" }));
  expect(await screen.findByRole("button", { name: "Selecionar fluxo ou agente" })).toHaveTextContent("Padrão");
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Modelo atual");
  expect(setModel).toHaveBeenCalledTimes(1);
});
function worker(thought: string): WorkflowAgent {
  return { id: "builder1", parentId: "main", role: "builder", title: "Construir integração", status: "running", createdAt: 1, updatedAt: 2, startedAt: 1, durationMs: 1000, currentThought: thought, attempts: 1, options: savedTurn().options, beadId: null, handoff: null, error: null, activeTurnId: "sub1", pendingApproval: null, pendingQuestion: null };
}

it("pairs with a named device and explains a used or expired QR", async () => {
  const user = userEvent.setup(); const { client } = fixtures();
  const pair = vi.spyOn(client, "pair").mockRejectedValue(new RemoteError("pairing_expired", "Expired"));
  render(<RemoteApp client={client} pairingToken="one-time-secret" />);
  await user.clear(screen.getByRole("textbox", { name: "Nome do celular" })); await user.type(screen.getByRole("textbox", { name: "Nome do celular" }), "Meu iPhone");
  await user.click(screen.getByRole("button", { name: "Conectar celular" }));
  expect(pair).toHaveBeenCalledWith("one-time-secret", "Meu iPhone");
  expect(await screen.findByText("Este QR expirou ou já foi usado. Gere um novo QR no computador.")).toBeVisible();
});

it("ignores desktop selection and preserves the phone draft across independent navigation", async () => {
  const user = userEvent.setup(); const { client, chat, mutate } = fixtures();
  render(<RemoteApp client={client} />);
  await screen.findByRole("button", { name: "Pessoal" }); expect(chat).not.toHaveBeenCalled();
  await openFirst(user); await screen.findByText("Mensagem c1");
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Rascunho local");
  await user.click(screen.getByRole("button", { name: "Voltar" }));
  await user.click(screen.getByRole("button", { name: "Primeira conversa" }));
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue("Rascunho local");
  await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
  await waitFor(() => expect(mutate).toHaveBeenCalledWith("message", { conversationId: "c1", content: "Rascunho local", options: remoteTurnOptions("standard", { ...savedTurn().options, executor: "jarvis" }, savedTurn().options) }));
  expect(chat.mock.calls.every(([id]) => id === "c1")).toBe(true);
  await waitFor(() => expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue(""));
});

it("prioritizes global pending chats and shows tasks and the Beads epic instead of file paths", async () => {
  const user = userEvent.setup(); const { client, library, chat, beads } = fixtures();
  library.runtime = [{ conversationId: "c2", revision: 2, activeTurnId: "turn1", compacting: false, attention: [{ kind: "question", agentId: "main" }] }];
  const current = bundle("c2"); current.chat.activeTurnId = "turn1"; current.chat.turns[0].status = "running";
  current.chat.turns[0].tasks = [{ id: "task1", title: "Revisar contrato", status: "in_progress" }];
  current.chat.fileChanges = [{ path: "src/remote/client.ts", additions: 12, deletions: 2, base: "git" }];
  beads.mockResolvedValue({ issues: [{ id: "project-1", title: "Sincronização confiável", status: "in_progress", issueType: "epic", parentId: null }, { id: "project-1.1", title: "Validar PostgreSQL", status: "closed", issueType: "task", parentId: "project-1" }] });
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />);
  await screen.findByRole("button", { name: "Trabalho" });
  expect(beads).not.toHaveBeenCalled();
  await user.click(await screen.findByRole("button", { name: /Conversa do trabalho.*Pergunta/ }));
  expect(await screen.findByText("Em execução")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Abrir Inspector" }));
  expect(await screen.findByText("Revisar contrato")).toBeVisible();
  expect(await screen.findByText("Sincronização confiável")).toBeVisible();
  expect(screen.getByText("Validar PostgreSQL")).toBeVisible();
  expect(screen.getByLabelText("1 de 1 tarefas do épico concluídas")).toBeVisible();
  expect(screen.queryByText("Arquivos alterados")).not.toBeInTheDocument();
  expect(screen.queryByText("src/remote/client.ts")).not.toBeInTheDocument();
  expect(chat).toHaveBeenCalledWith("c2", expect.any(AbortSignal));
  expect(beads).toHaveBeenCalledWith("c2", expect.any(AbortSignal));
});

it("refreshes on focus, network return and foregrounding while retaining a draft", async () => {
  const user = userEvent.setup(); const { client, list } = fixtures();
  render(<RemoteApp client={client} />); await openFirst(user); await screen.findByText("Mensagem c1");
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Ainda aqui");
  for (const event of ["focus", "online"]) { const count = list.mock.calls.length; act(() => { window.dispatchEvent(new Event(event)); }); await waitFor(() => expect(list.mock.calls.length).toBeGreaterThan(count)); }
  const count = list.mock.calls.length; act(() => { document.dispatchEvent(new Event("visibilitychange")); }); await waitFor(() => expect(list.mock.calls.length).toBeGreaterThan(count));
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue("Ainda aqui");
});

it("recovers the saved device session when the first read fails offline", async () => {
  const { client } = fixtures();
  vi.mocked(client.session).mockRejectedValueOnce(new RemoteError("offline", "Sem rede"));
  render(<RemoteApp client={client} />);
  expect(await screen.findByText("Reconectando ao computador…")).toBeVisible();
  expect(screen.queryByText("Pareie novamente pelo Jarvis no computador.")).not.toBeInTheDocument();
  act(() => { window.dispatchEvent(new Event("online")); });
  expect(await screen.findByRole("button", { name: "Pessoal" })).toBeVisible();
  expect(client.session).toHaveBeenCalledTimes(2);
});

it("drops stale responses after navigation and reconnects to the chosen conversation", async () => {
  const user = userEvent.setup(); const { client, chat } = fixtures();
  let release: ((value: RemoteChat) => void) | undefined;
  chat.mockImplementation(id => id === "c1" ? new Promise(resolve => { release = resolve; }) : Promise.resolve(bundle(id)));
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.click(screen.getByRole("button", { name: "Voltar" })); await user.click(screen.getByRole("button", { name: "Voltar" })); await user.click(screen.getByRole("button", { name: "Voltar" }));
  await user.click(screen.getByRole("button", { name: "Trabalho" })); await user.click(screen.getByRole("button", { name: /Outro projeto/ })); await user.click(screen.getByRole("button", { name: "Conversa do trabalho" }));
  expect(await screen.findByText("Mensagem c2")).toBeVisible();
  await act(async () => { release?.(bundle("c1")); });
  expect(screen.queryByText("Mensagem c1")).not.toBeInTheDocument(); expect(screen.getByText("Mensagem c2")).toBeVisible();
});

it("requires a new pairing after a device is revoked", async () => {
  const { client, list } = fixtures(); list.mockRejectedValue(new RemoteError("session_expired", "Revoked", 401));
  render(<RemoteApp client={client} />);
  expect(await screen.findByText("Este acesso expirou ou foi revogado. Leia um novo QR no computador.")).toBeVisible();
  expect(screen.queryByRole("textbox", { name: "Mensagem" })).not.toBeInTheDocument();
});

it("stops polling in the background and resyncs immediately on return", async () => {
  const { client, list } = fixtures();
  const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  render(<RemoteApp client={client} />); await screen.findByRole("button", { name: "Pessoal" });
  visibility.mockReturnValue("hidden"); act(() => { document.dispatchEvent(new Event("visibilitychange")); });
  const count = list.mock.calls.length; vi.useFakeTimers(); await act(async () => { await vi.advanceTimersByTimeAsync(6_000); });
  expect(list).toHaveBeenCalledTimes(count);
  visibility.mockReturnValue("visible"); await act(async () => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(list.mock.calls.length).toBeGreaterThan(count);
});

it("resyncs after a network failure without replaying a message or losing its draft", async () => {
  const user = userEvent.setup(); const { client, chat, mutate } = fixtures();
  render(<RemoteApp client={client} />); await openFirst(user); await screen.findByText("Mensagem c1");
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Conservar esta mensagem");
  chat.mockRejectedValueOnce(new RemoteError("offline", "Conexão perdida"));
  act(() => { window.dispatchEvent(new Event("focus")); });
  expect(await screen.findByText("Reconectando ao computador")).toBeVisible(); expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  act(() => { window.dispatchEvent(new Event("online")); });
  await waitFor(() => expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeEnabled());
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue("Conservar esta mensagem"); expect(mutate).not.toHaveBeenCalled();
});

it("opens global pending conversations while reading a different chat", async () => {
  const user = userEvent.setup(); const { client, library } = fixtures();
  library.runtime = [{ conversationId: "c2", revision: 1, activeTurnId: "turn1", compacting: false, attention: [{ kind: "approval", agentId: "main" }] }];
  render(<RemoteApp client={client} />); await openFirst(user); await screen.findByText("Mensagem c1");
  await user.click(screen.getByRole("button", { name: /Precisa de você 1/ }));
  await user.click(await screen.findByRole("button", { name: /Conversa do trabalho.*Permissão/ }));
  expect(await screen.findByText("Mensagem c2")).toBeVisible();
});

it("focuses new subagent questions and returns to the pinned conversation only when requested", async () => {
  const user = userEvent.setup(); const { client, chat } = fixtures();
  const current = bundle(); current.chat.activeTurnId = "turn1";
  current.workflow = { conversationId: "c1", revision: 1, flow: "planned", agents: [] };
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />); await openFirst(user); await screen.findByText("Mensagem c1");
  const transcript = screen.getByLabelText("Conversa");
  Object.defineProperty(transcript, "scrollHeight", { configurable: true, value: 600 });
  const workflow: NonNullable<RemoteChat["workflow"]> = { ...current.workflow, revision: 2, agents: [{ id: "designer1", parentId: "main", role: "designer", title: "Designer", status: "waiting", createdAt: 1, updatedAt: 1, startedAt: 1, durationMs: 0, currentThought: null, attempts: 1, options: savedTurn().options, beadId: null, handoff: null, error: null, activeTurnId: "sub1", pendingApproval: null, pendingQuestion: { turnId: "sub1", toolId: "q1", questions: [{ id: "color", question: "Qual cor?", options: [] }] } }] };
  chat.mockResolvedValue({ ...current, workflow });
  act(() => { window.dispatchEvent(new Event("focus")); });
  await screen.findByText("Qual cor?");
  expect(screen.getByRole("region", { name: "Decisão pendente" })).toBeVisible();
  expect(transcript).not.toBeVisible();
  expect(screen.queryByRole("textbox", { name: "Mensagem" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Ver conversa" }));
  expect(transcript.scrollTop).toBe(600);
  await user.click(screen.getByRole("button", { name: "Abrir Inspector" }));
  expect(within(await screen.findByRole("dialog")).getByText("Precisa de você")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Fechar Inspector" }));
  transcript.scrollTop = 0; fireEvent.scroll(transcript);
  chat.mockResolvedValue({ ...current, workflow: { ...workflow, revision: 3 } });
  act(() => { window.dispatchEvent(new Event("focus")); });
  await waitFor(() => expect(chat).toHaveBeenCalledTimes(3));
  expect(transcript.scrollTop).toBe(0);
});

it("gives publication the chat area while preserving both drafts and requiring an explicit decision", async () => {
  const user = userEvent.setup(); const { client, chat, mutate } = fixtures();
  const current = bundle(); current.chat.activeTurnId = "turn1";
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Rascunho da conversa");
  const proposal: NonNullable<RemoteChat["chat"]["pendingAuthoring"]> = {
    turnId: "turn1", toolId: "publish1", action: "publish", catalogRevision: null, summary: "Publicar a correção.", agentReferences: [],
    target: { kind: "publication", after: { summary: "Publicar a correção.", authorization: null, repositories: [{ path: ".", files: ["src/App.tsx"], reset: null, branch: "fix/mobile", commitMessage: "fix: mobile layout", sync: "none", push: "normal", pullRequest: null }] } },
  };
  const waiting = { ...current, chat: { ...current.chat, revision: 2, pendingAuthoring: proposal } };
  chat.mockResolvedValue(waiting); act(() => { window.dispatchEvent(new Event("focus")); });
  const decision = await screen.findByRole("region", { name: "Decisão pendente" });
  expect(screen.queryByRole("textbox", { name: "Mensagem" })).not.toBeInTheDocument();
  expect(screen.getByLabelText("Conversa")).not.toBeVisible();
  await user.type(within(decision).getByRole("textbox", { name: "Orientação para o agente (opcional)" }), "Revise o título");
  await user.click(screen.getByRole("button", { name: "Ver conversa" }));
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue("Rascunho da conversa");
  expect(decision).not.toBeVisible();
  chat.mockResolvedValue({ ...waiting, workflow: { conversationId: "c1", revision: 1, flow: "planned", agents: [worker("Outra atividade")] } });
  act(() => { window.dispatchEvent(new Event("focus")); });
  await screen.findByText("Construtor");
  expect(decision).not.toBeVisible();
  await user.click(screen.getByRole("button", { name: "Responder" }));
  expect(within(decision).getByRole("textbox", { name: "Orientação para o agente (opcional)" })).toHaveValue("Revise o título");
  expect(mutate).not.toHaveBeenCalled();
  await user.click(within(decision).getByRole("button", { name: "Enviar para revisão" }));
  expect(mutate).toHaveBeenCalledWith("validation", { conversationId: "c1", agentId: "main", kind: "publication", decision: { turnId: "turn1", toolId: "publish1", approved: true, note: "Revise o título" } });
});

it("expands a working composer for writing, keeps live details optional and compacts after sending", async () => {
  const user = userEvent.setup(); const { client, chat, mutate } = fixtures();
  const current = bundle(); current.chat.activeTurnId = "turn1";
  current.workflow = { conversationId: "c1", revision: 1, flow: "planned", agents: [worker("Verificando o feedback da revisão")] };
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />); await openFirst(user);
  const editor = screen.getByRole("textbox", { name: "Mensagem" });
  expect(editor).toHaveAttribute("rows", "1");
  const details = screen.getByRole("button", { name: "Detalhes da atividade atual" });
  expect(details).toHaveAttribute("aria-expanded", "false");
  await user.click(details);
  expect(details).toHaveAttribute("aria-expanded", "true");
  expect(mutate).not.toHaveBeenCalled();
  await user.type(editor, "Use PostgreSQL");
  expect(editor).toHaveAttribute("rows", "2");
  await user.click(screen.getByRole("button", { name: "Enviar mensagem para a fila" }));
  expect(mutate).toHaveBeenCalledWith("message", { conversationId: "c1", content: "Use PostgreSQL", options: remoteTurnOptions("standard", { ...savedTurn().options, executor: "jarvis" }, savedTurn().options) });
  await waitFor(() => expect(editor).toHaveValue(""));
  expect(editor).toHaveAttribute("rows", "1");
});

it("propagates colored running and attention indicators through workspace, project and chat cards", async () => {
  const user = userEvent.setup(); const { client, library, list } = fixtures();
  library.runtime = [{ conversationId: "c1", revision: 1, activeTurnId: "turn1", compacting: false, status: "running", attention: [] }];
  render(<RemoteApp client={client} />);
  const workspace = await screen.findByRole("button", { name: "Pessoal" });
  expect(workspace.closest('[data-slot="card"]')).toHaveAttribute("data-status", "running");
  expect(within(workspace.closest('[data-slot="card"]') as HTMLElement).getByText("1 em execução")).toBeVisible();
  await user.click(workspace);
  const project = screen.getByRole("button", { name: "Jarvis" });
  expect(project.closest('[data-slot="card"]')).toHaveAttribute("data-status", "running");
  expect(screen.queryByText("/projects/jarvis")).not.toBeInTheDocument();
  await user.click(project);
  const conversation = screen.getByRole("button", { name: "Primeira conversa" });
  expect(conversation.closest('[data-slot="card"]')).toHaveAttribute("data-status", "running");
  library.runtime = [{ ...library.runtime[0], status: "waiting", attention: [{ kind: "question", agentId: "main" }] }];
  list.mockResolvedValue({ ...library });
  act(() => { window.dispatchEvent(new Event("focus")); });
  await waitFor(() => expect(conversation.closest('[data-slot="card"]')).toHaveAttribute("data-status", "waiting"));
});

it("shows current worker thought even after the root's last answer and accepts same-revision live updates", async () => {
  const user = userEvent.setup(); const { client, chat } = fixtures();
  const current = bundle();
  current.workflow = { conversationId: "c1", revision: 4, flow: "planned", agents: [worker("Conectando ao PostgreSQL local")] };
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />); await openFirst(user);
  const live = await screen.findByLabelText("Atividade atual");
  expect(within(live).getByText("Conectando ao PostgreSQL local")).toBeVisible();
  expect(within(live).getByText("Construtor")).toBeVisible();
  chat.mockResolvedValue({ ...current, workflow: { ...current.workflow, agents: [worker("Validando sincronização de obras")] } });
  act(() => { window.dispatchEvent(new Event("focus")); });
  expect(await within(live).findByText("Validando sincronização de obras")).toBeVisible();
  expect(within(live).queryByText("Conectando ao PostgreSQL local")).not.toBeInTheDocument();
});

it("advances streamed chat and completes actions while an unrelated library read is stalled", async () => {
  const user = userEvent.setup(); const { client, list, chat, mutate } = fixtures();
  render(<RemoteApp client={client} />); await openFirst(user); await screen.findByText("Mensagem c1");
  list.mockImplementation(() => new Promise(() => {}));
  const current = bundle(); current.chat.revision = 5;
  current.chat.turns[0].steps[0].text = "Acabei de verificar o banco local.";
  chat.mockResolvedValue(current);
  act(() => { window.dispatchEvent(new Event("focus")); });
  expect(await screen.findByText("Acabei de verificar o banco local.")).toBeVisible();
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Continue");
  await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
  await waitFor(() => expect(mutate).toHaveBeenCalledWith("message", { conversationId: "c1", content: "Continue", options: remoteTurnOptions("standard", { ...savedTurn().options, executor: "jarvis" }, savedTurn().options) }));
  await waitFor(() => expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue(""));
  expect(screen.getByRole("button", { name: "Desconectar celular" })).toBeEnabled();
});

it("reconciles root and workflow revisions independently without replacing a newer transcript", async () => {
  const user = userEvent.setup(); const { client, chat } = fixtures();
  const current = bundle(); current.chat.revision = 8; current.chat.turns[0].steps[0].text = "Resposta mais recente";
  current.workflow = { conversationId: "c1", revision: 2, flow: "planned", agents: [worker("Primeira etapa")] };
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />); await openFirst(user); await screen.findByText("Resposta mais recente");
  const stale = bundle(); stale.chat.revision = 7; stale.chat.turns[0].steps[0].text = "Resposta anterior";
  stale.workflow = { ...current.workflow, revision: 3, agents: [worker("Segunda etapa")] };
  chat.mockResolvedValue(stale);
  act(() => { window.dispatchEvent(new Event("focus")); });
  expect(await screen.findByText("Segunda etapa")).toBeVisible();
  expect(screen.getByText("Resposta mais recente")).toBeVisible();
  expect(screen.queryByText("Resposta anterior")).not.toBeInTheDocument();
});

it("keeps delivered send-now messages in their transcript position and discloses reasoning", async () => {
  const user = userEvent.setup(); const { client, chat } = fixtures();
  const current = bundle(); const turn = current.chat.turns[0];
  const step = turn.steps[0];
  turn.steps = [{ ...step, summary: "Vou verificar a origem dos dados", text: "Antes da instrução" }, { ...step, summary: "", text: "Depois da instrução" }];
  turn.auxiliaryMessages = [{ id: "now-1", content: "Use o banco PostgreSQL", options: turn.options, parts: [], afterStep: 1 }];
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />); await openFirst(user);
  const transcript = await screen.findByLabelText("Conversa");
  await screen.findByText("Depois da instrução");
  const content = transcript.textContent ?? "";
  expect(content.indexOf("Antes da instrução")).toBeLessThan(content.indexOf("Use o banco PostgreSQL"));
  expect(content.indexOf("Use o banco PostgreSQL")).toBeLessThan(content.indexOf("Depois da instrução"));
  expect(screen.getAllByText("Use o banco PostgreSQL")).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Raciocínio" }));
  expect(await screen.findByText("Vou verificar a origem dos dados")).toBeVisible();
});

it("shows tracked tasks once while preserving different direct tasks", async () => {
  const user = userEvent.setup(); const { client, chat, beads } = fixtures();
  const current = bundle();
  current.chat.turns[0].tasks = [{ id: "project-1.1", title: "Validar o banco", status: "in_progress" }, { id: "local-2", title: "Revisar a navegação", status: "pending" }];
  chat.mockResolvedValue(current);
  beads.mockResolvedValue({ issues: [{ id: "project-1", title: "Integração", status: "in_progress", issueType: "epic", parentId: null }, { id: "project-1.1", title: "Validar o banco", status: "in_progress", issueType: "task", parentId: "project-1" }] });
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.click(screen.getByRole("button", { name: "Abrir Inspector" }));
  expect(await screen.findByText("Integração")).toBeVisible();
  expect(screen.getAllByText("Validar o banco")).toHaveLength(1);
  expect(screen.getByText("Revisar a navegação")).toBeVisible();
});

it("never shows the previous project's Beads plan while loading another conversation", async () => {
  const user = userEvent.setup(); const { client, beads } = fixtures();
  beads.mockImplementation(id => id === "c1" ? Promise.resolve({ issues: [{ id: "jarvis-1", title: "Plano exclusivo do Jarvis", status: "open", issueType: "epic", parentId: null }] }) : new Promise(() => {}));
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.click(screen.getByRole("button", { name: "Abrir Inspector" }));
  expect(await screen.findByText("Plano exclusivo do Jarvis")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Fechar Inspector" }));
  for (let index = 0; index < 3; index++) await user.click(screen.getByRole("button", { name: "Voltar" }));
  await user.click(screen.getByRole("button", { name: "Trabalho" })); await user.click(screen.getByRole("button", { name: "Outro projeto" })); await user.click(screen.getByRole("button", { name: "Conversa do trabalho" }));
  await screen.findByText("Mensagem c2"); await user.click(screen.getByRole("button", { name: "Abrir Inspector" }));
  expect(await screen.findByRole("status", { name: "Carregando plano do projeto" })).toBeVisible();
  expect(screen.queryByText("Plano exclusivo do Jarvis")).not.toBeInTheDocument();
});

it("keeps the focused queue editor visible after the phone keyboard resizes the viewport", async () => {
  const user = userEvent.setup(); const { client, chat } = fixtures();
  const viewport = Object.assign(new EventTarget(), { height: 844 });
  vi.stubGlobal("visualViewport", viewport);
  const scroll = vi.spyOn(Element.prototype, "scrollIntoView").mockImplementation(() => {});
  const current = bundle(); current.chat.queuedMessages = [{ id: "queued-1", content: "Uma mensagem para revisar", options: current.chat.turns[0].options, parts: [] }];
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />); await openFirst(user);
  await user.click(await screen.findByRole("button", { name: "Editar mensagem 1" }));
  const editor = screen.getByRole("textbox", { name: "Editar mensagem 1" });
  expect(editor).toHaveFocus();
  scroll.mockClear(); viewport.height = 640;
  act(() => { viewport.dispatchEvent(new Event("resize")); });
  expect(document.documentElement.style.getPropertyValue("--remote-height")).toBe("640px");
  await waitFor(() => expect(scroll).toHaveBeenCalledWith({ block: "nearest", inline: "nearest" }));
  expect(scroll.mock.instances[0]).toBe(editor);
  expect(editor).toHaveValue("Uma mensagem para revisar");
});

it("does not jump the transcript when resizing without an editor or with only the composer focused", async () => {
  const user = userEvent.setup(); const { client } = fixtures();
  const scroll = vi.spyOn(Element.prototype, "scrollIntoView").mockImplementation(() => {});
  let callback: FrameRequestCallback | undefined;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(value => { callback = value; return 7; });
  const cancelFrame = vi.spyOn(window, "cancelAnimationFrame");
  const { unmount } = render(<RemoteApp client={client} />); await openFirst(user);
  act(() => { window.dispatchEvent(new Event("resize")); callback?.(0); });
  expect(scroll).not.toHaveBeenCalled();
  await user.click(screen.getByRole("textbox", { name: "Mensagem" }));
  act(() => { window.dispatchEvent(new Event("resize")); callback?.(0); });
  expect(scroll).not.toHaveBeenCalled();
  act(() => { window.dispatchEvent(new Event("resize")); });
  unmount(); expect(cancelFrame).toHaveBeenCalledWith(7);
});
