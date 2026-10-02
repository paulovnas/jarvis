import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { populatedLibrary } from "@/test/library-fixtures";
import { RemoteClient, RemoteError, type RemoteChat, type RemoteLibrary } from "./client";
import { RemoteApp } from "./RemoteApp";

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });
const bundle = (id = "c1"): RemoteChat => ({ chat: { ...emptyChat(id), turns: [{ ...savedTurn(), user: `Mensagem ${id}` }] }, workflow: null, options: null });
function fixtures() {
  const client = new RemoteClient(); const library: RemoteLibrary = { library: populatedLibrary(), runtime: [] };
  vi.spyOn(client, "session").mockResolvedValue({ deviceId: "phone", name: "Celular" });
  const list = vi.spyOn(client, "library").mockResolvedValue(library);
  const chat = vi.spyOn(client, "chat").mockImplementation(async id => bundle(id));
  const mutate = vi.spyOn(client, "mutate").mockResolvedValue({ ok: true });
  return { client, library, list, chat, mutate };
}
async function openFirst(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Pessoal" }));
  await user.click(screen.getByRole("button", { name: /Jarvis.*projects/ }));
  await user.click(screen.getByRole("button", { name: "Primeira conversa" }));
  await screen.findByRole("textbox", { name: "Mensagem" });
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
  await waitFor(() => expect(mutate).toHaveBeenCalledWith("message", { conversationId: "c1", content: "Rascunho local" }));
  expect(chat.mock.calls.every(([id]) => id === "c1")).toBe(true);
  await waitFor(() => expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveValue(""));
});

it("prioritizes global pending chats and exposes live status, tasks and changed files", async () => {
  const user = userEvent.setup(); const { client, library, chat } = fixtures();
  library.runtime = [{ conversationId: "c2", revision: 2, activeTurnId: "turn1", compacting: false, attention: [{ kind: "question", agentId: "main" }] }];
  const current = bundle("c2"); current.chat.activeTurnId = "turn1"; current.chat.turns[0].status = "running";
  current.chat.turns[0].tasks = [{ id: "task1", title: "Revisar contrato", status: "in_progress" }];
  current.chat.fileChanges = [{ path: "src/remote/client.ts", additions: 12, deletions: 2, base: "git" }];
  chat.mockResolvedValue(current);
  render(<RemoteApp client={client} />);
  await user.click(await screen.findByRole("button", { name: /Conversa do trabalho.*Pergunta/ }));
  expect(await screen.findByText("Em execução")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Abrir Inspector" }));
  expect(await screen.findByText("Revisar contrato")).toBeVisible(); expect(screen.getByText("src/remote/client.ts")).toBeVisible();
  expect(chat).toHaveBeenCalledWith("c2", expect.any(AbortSignal));
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
  const user = userEvent.setup(); const { client, list, mutate } = fixtures();
  render(<RemoteApp client={client} />); await openFirst(user); await screen.findByText("Mensagem c1");
  await user.type(screen.getByRole("textbox", { name: "Mensagem" }), "Conservar esta mensagem");
  list.mockRejectedValueOnce(new RemoteError("offline", "Conexão perdida"));
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

it("keeps new subagent questions visible while pinned and reports waiting in Inspector", async () => {
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
