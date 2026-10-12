import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { expect, it, vi } from "vitest";
import type { TurnOptions } from "@/core/chat";
import { useImpeccableLive, type ImpeccableLiveTask } from "@/hooks/use-impeccable-live";
import { chatOptions } from "@/test/chat-fixtures";
import { DesignerPanel, type DesignerSession } from "./DesignerPanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

function task(status: ImpeccableLiveTask["status"] = "pending", content = "Reposicione a versão") : ImpeccableLiveTask {
  return { requestId: `live-request-${status}`, actionId: "upstream-session", revision: 3, content, status, beadId: "jarvis-123", turnId: status === "running" ? "turn-live" : null, createdAt: 1791568800000 };
}
function session(tasks: ImpeccableLiveTask[] = []): DesignerSession {
  return {
    conversationId: "c1", canResume: true, attention: false, onResume: vi.fn(async () => {}), onShowChat: vi.fn(),
    live: {
      status: { conversationId: "c1", state: "ready", phase: "running", actionId: "upstream-session", model: "modelo-atual", provider: "conta-atual", step: 4, activity: "Conferindo a posição da versão", elapsedMs: 13000, tasks, url: "http://localhost:5173/", tabId: "page-1", error: null, setupNeeded: null },
      active: true, loaded: true, busy: false, toggle: vi.fn(async () => {}), resume: vi.fn(async () => {}),
      editTask: vi.fn(async () => null), removeTask: vi.fn(async () => null), resumeTask: vi.fn(async () => null), cancelAction: vi.fn(async () => null),
    },
  };
}

it("shows actual integration progress and exposes actions only for eligible Live requests", () => {
  const state = session([task(), task("running", "Ajuste o botão"), task("paused", "Revise o espaçamento"), task("completed", "Título revisado")]);
  render(<DesignerPanel session={state} />);
  const progress = screen.getByRole("status", { name: "Estado do Designer" });
  expect(progress).toHaveTextContent("conta-atual · modelo-atual");
  expect(progress).toHaveTextContent("Etapa 4 · 13s");
  expect(progress).toHaveTextContent("Conferindo a posição da versão");
  expect(screen.getByRole("region", { name: "Alterações em andamento" })).toHaveTextContent("Ajuste o botão");
  expect(screen.getByRole("region", { name: "Alterações pendentes" })).toHaveTextContent("Revise o espaçamento");
  expect(screen.getByRole("button", { name: "Cancelar alteração: Ajuste o botão" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Editar alteração: Ajuste o botão" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Retomar alteração: Revise o espaçamento" })).toBeEnabled();
  expect(state.onResume).not.toHaveBeenCalled();
  expect(state.live.resumeTask).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: /Título revisado/ })).not.toBeInTheDocument();
});

it("edits a pending request inline, preserving the text and captured revision when the save fails", async () => {
  const user = userEvent.setup();
  const original = task();
  const state = session([original]);
  vi.mocked(state.live.editTask).mockResolvedValueOnce("Esta alteração foi atualizada. Confira o pedido e tente novamente.").mockResolvedValueOnce(null);
  const view = render(<DesignerPanel session={state} />);
  await user.click(screen.getByRole("button", { name: "Editar alteração: Reposicione a versão" }));
  const input = screen.getByRole("textbox", { name: "Pedido da alteração" });
  await user.clear(input);
  await user.type(input, "Coloque a versão ao lado do nome");
  view.rerender(<DesignerPanel session={{ ...state, live: { ...state.live, status: { ...state.live.status, tasks: [{ ...original, revision: 4 }] } } }} />);
  await user.click(screen.getByRole("button", { name: "Salvar" }));
  expect(state.live.editTask).toHaveBeenCalledWith(original, "Coloque a versão ao lado do nome");
  expect(screen.getByRole("alert")).toHaveTextContent("Esta alteração foi atualizada");
  expect(input).toHaveValue("Coloque a versão ao lado do nome");
  expect(state.live.editTask).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "Salvar" }));
  expect(state.live.editTask).toHaveBeenLastCalledWith({ ...original, revision: 4 }, "Coloque a versão ao lado do nome");
  expect(screen.queryByRole("textbox", { name: "Pedido da alteração" })).not.toBeInTheDocument();
});

it("does not save an edited request that has already started and retains the user's draft", async () => {
  const user = userEvent.setup();
  const original = task();
  const state = session([original]);
  const view = render(<DesignerPanel session={state} />);
  await user.click(screen.getByRole("button", { name: "Editar alteração: Reposicione a versão" }));
  await user.clear(screen.getByRole("textbox", { name: "Pedido da alteração" }));
  await user.type(screen.getByRole("textbox", { name: "Pedido da alteração" }), "Não perca este rascunho");
  view.rerender(<DesignerPanel session={{ ...state, live: { ...state.live, status: { ...state.live.status, tasks: [{ ...original, status: "running", revision: 4 }] } } }} />);
  expect(screen.getByRole("textbox", { name: "Pedido da alteração" })).toHaveValue("Não perca este rascunho");
  expect(screen.getByRole("button", { name: "Salvar" })).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("já saiu da fila");
  expect(state.live.editTask).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Cancelar edição" }));
  expect(screen.queryByRole("textbox", { name: "Pedido da alteração" })).not.toBeInTheDocument();
});

it("removes and explicitly resumes preserved pending requests while Live is off", async () => {
  const user = userEvent.setup();
  const pending = task();
  const paused = task("paused", "Revise o espaçamento");
  const state = session([pending, paused]);
  state.live = { ...state.live, active: false, status: { ...state.live.status, state: "off", phase: "idle", actionId: null } };
  render(<DesignerPanel session={state} />);
  expect(screen.getByRole("status", { name: "Estado do Designer" })).toHaveTextContent("Live desligado");
  await user.click(screen.getByRole("button", { name: "Remover alteração: Reposicione a versão" }));
  expect(state.live.removeTask).toHaveBeenCalledWith(pending);
  await user.click(screen.getByRole("button", { name: "Retomar alteração: Revise o espaçamento" }));
  expect(state.live.resumeTask).toHaveBeenCalledWith(paused);
  expect(state.live.toggle).not.toHaveBeenCalled();
});

it("cancels only the active Live action and leaves pending requests untouched", async () => {
  const user = userEvent.setup();
  const state = session([task(), task("running", "Ajuste o botão")]);
  render(<DesignerPanel session={state} />);
  await user.click(screen.getByRole("button", { name: "Cancelar alteração: Ajuste o botão" }));
  expect(state.live.cancelAction).toHaveBeenCalledWith("upstream-session", "live-request-running", 3);
  expect(state.live.removeTask).not.toHaveBeenCalled();
  expect(state.live.toggle).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Editar alteração: Reposicione a versão" })).toBeEnabled();
});

it("removes blocked requests and resumes them with the newly selected model without allowing edits", async () => {
  const user = userEvent.setup();
  const blocked = task("blocked");
  const retained = { ...task("blocked", "Revise o espaçamento"), requestId: "other-session:1", actionId: "other-session" };
  const initial = { ...session([blocked, retained]).live.status, state: "setup" as const, phase: "blocked" as const, error: "O provedor atingiu o limite." };
  vi.mocked(listen).mockResolvedValue(() => {});
  vi.mocked(invoke).mockReset().mockImplementation(async command => command === "get_impeccable_live" ? initial : command === "remove_impeccable_live_task" ? { ...initial, tasks: [retained] } : { ...initial, state: "ready", phase: "waiting", error: null, tasks: [{ ...retained, status: "pending", revision: 4 }] });
  function Connected({ options }: { options: TurnOptions }) {
    const live = useImpeccableLive("c1", true, { options, canResume: true });
    return <DesignerPanel session={{ ...session(), live }} />;
  }
  const view = render(<Connected options={chatOptions} />);
  expect(await screen.findByRole("button", { name: "Retomar alteração: Reposicione a versão" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: /Editar alteração/ })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Cancelar alteração/ })).not.toBeInTheDocument();
  expect(invoke).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "Remover alteração: Reposicione a versão" }));
  expect(invoke).toHaveBeenLastCalledWith("remove_impeccable_live_task", { conversationId: "c1", requestId: blocked.requestId, expectedRevision: blocked.revision });
  expect(screen.queryByText(blocked.content)).not.toBeInTheDocument();
  const options = { ...chatOptions, account: "nova-conta", model: "novo-modelo" };
  view.rerender(<Connected options={options} />);
  await user.click(screen.getByRole("button", { name: "Retomar alteração: Revise o espaçamento" }));
  expect(invoke).toHaveBeenCalledWith("resume_impeccable_live_task", { conversationId: "c1", requestId: retained.requestId, expectedRevision: retained.revision, options });
  expect(within(screen.getByRole("region", { name: "Alterações pendentes" })).getByText(retained.content)).toBeVisible();
});

it("keeps errors and attention recovery available without starting work when the panel opens", async () => {
  const user = userEvent.setup();
  const state = session([task("paused")]);
  state.attention = true;
  state.canResume = false;
  state.live.status = { ...state.live.status, state: "setup", phase: "blocked", error: "O modelo selecionado atingiu o limite." };
  const view = render(<DesignerPanel session={state} />);
  expect(screen.getByRole("status", { name: "Estado do Designer" })).toHaveTextContent("O modelo selecionado atingiu o limite.");
  expect(screen.getByRole("button", { name: "Retomar Live" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Retomar alteração: Reposicione a versão" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Responder no chat" }));
  expect(state.onShowChat).toHaveBeenCalledOnce();
  view.rerender(<DesignerPanel session={{ ...state, attention: false, canResume: true }} />);
  await user.click(screen.getByRole("button", { name: "Retomar Live" }));
  expect(state.onResume).toHaveBeenCalledOnce();
  expect(within(screen.getByRole("region", { name: "Alterações pendentes" })).getByRole("button", { name: /Retomar alteração/ })).toBeEnabled();
});

it.each(["waiting", "running"] as const)("shows %s setup preparation without inviting another Live retry", phase => {
  const state = session();
  state.live.status = { ...state.live.status, state: "setup", phase, actionId: null, activity: "Preparando a página local", setupNeeded: { error: "page_unavailable" } };
  render(<DesignerPanel session={state} />);
  const progress = screen.getByRole("status", { name: "Estado do Designer" });
  expect(progress).toHaveTextContent(phase === "waiting" ? "Aguardando agente" : "Em andamento");
  expect(progress).toHaveTextContent("Preparando a página local");
  expect(progress).not.toHaveTextContent("Precisa de atenção");
  expect(screen.queryByRole("button", { name: "Retomar Live" })).not.toBeInTheDocument();
  expect(state.onResume).not.toHaveBeenCalled();
});

it("uses a structural loading state and useful empty guidance", () => {
  const state = session(); state.live.loaded = false;
  const view = render(<DesignerPanel session={state} />);
  expect(screen.getByRole("status", { name: "Carregando integração Designer" })).toBeVisible();
  view.rerender(<DesignerPanel />);
  expect(screen.getByText("Abra uma conversa para acompanhar as alterações.")).toBeVisible();
});

it("keeps a large completed history collapsed and renders only the six most recent requests", async () => {
  const user = userEvent.setup();
  const completed = Array.from({ length: 1024 }, (_, index) => ({ ...task("completed", `Alteração ${index}`), requestId: `session:${index}` }));
  render(<DesignerPanel session={session(completed)} />);
  expect(screen.queryByRole("listitem")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Recentes/ }));
  expect(screen.getAllByRole("listitem")).toHaveLength(6);
  expect(screen.getByText("Alteração 1023")).toBeVisible();
  expect(screen.queryByText("Alteração 1017")).not.toBeInTheDocument();
});
