import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CompanionItem, CompanionSnapshot } from "@/core/companion";
import type { AccountUsage } from "@/core/provider-usage";
import { Companion } from "./Companion";
import type { RobotProps } from "./Robot";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./Robot", () => ({ Robot: ({ status, visible, hovered, dragging, expanded, walking, lookX, lookY }: RobotProps) =>
  <svg aria-hidden="true" data-state={status} data-visible={visible} data-hovered={hovered} data-dragging={dragging} data-expanded={expanded} data-walking={walking} data-look-x={lookX} data-look-y={lookY} />,
}));
const call = vi.mocked(invoke);
const events = new Map<string, (payload: unknown) => void>();
const stop = vi.fn();
const base: CompanionItem = {
  conversationId: "conversation-1", agentId: null, projectId: "project-1", projectName: "Portal",
  title: "Melhorar o relatório", role: "builder", status: "running", activity: "Verificando o relatório",
  durationMs: 120_000, activeSince: 990_000, updatedAt: 1_000_000, requiresConversation: false, attentionId: "turn-1/running", acknowledged: false,
};
let snapshot: CompanionSnapshot;
const accounts: AccountUsage[] = [{
  alias: "openai-codex-pessoal", fetchedAt: 1_000_000, email: null, plan: null, error: null, resetCredits: null,
  windows: [{ id: "window-1", group: "Codex", thirdParty: false, label: "5h", durationSeconds: 18_000, remainingPercent: 72, resetsAt: 1_200_000 }],
}];
async function expanded() {
  const user = userEvent.setup();
  render(<Companion />);
  await screen.findByRole("button", { name: /Abrir assistente Jarvis, 1 atividade/ });
  await user.click(screen.getByRole("button", { name: /Abrir assistente Jarvis/ }));
  await screen.findByRole("region", { name: "Assistente Jarvis" });
  return user;
}

describe("Desktop companion", () => {
  beforeEach(() => {
    snapshot = { items: [{ ...base }], truncated: false };
    events.clear(); stop.mockClear();
    vi.spyOn(Date, "now").mockReturnValue(1_000_000);
    vi.stubGlobal("PointerEvent", MouseEvent);
    vi.mocked(listen).mockReset().mockImplementation(async (name, callback) => {
      events.set(String(name), payload => callback({ event: String(name), id: 1, payload }));
      return stop;
    });
    let expandedState = false;
    call.mockReset().mockImplementation(async (command, args) => {
      if (command === "get_companion_snapshot") return snapshot;
      if (command === "get_companion_usage") return accounts;
      if (command === "set_companion_expanded") {
        const isExpanded = (args as { expanded: boolean }).expanded;
        expandedState = isExpanded;
        return { expanded: isExpanded, bubble: false, robotSide: "right", robotVertical: "bottom", width: isExpanded ? 420 : 96, height: isExpanded ? 520 : 112 };
      }
      if (command === "set_companion_bubble") {
        const bubble = (args as { visible: boolean }).visible && !expandedState;
        return { expanded: expandedState, bubble, robotSide: "right", robotVertical: "bottom", width: expandedState ? 420 : bubble ? 360 : 96, height: expandedState ? 520 : bubble ? 300 : 112 };
      }
      return true;
    });
  });
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it("starts compact with live activity and does not bootstrap the main app or read transcripts", async () => {
    render(<Companion />);
    const pet = await screen.findByRole("button", { name: /Abrir assistente Jarvis, 1 atividade/ });
    expect(pet).toHaveAttribute("aria-expanded", "false");
    expect(pet.querySelector("svg[data-state=running]")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Assistente Jarvis" })).not.toBeInTheDocument();
    expect(call.mock.calls.map(([command]) => command)).toEqual(expect.arrayContaining(["get_companion_snapshot", "set_companion_expanded"]));
    expect(call.mock.calls.some(([command]) => /bootstrap|transcript|get_provider/.test(command))).toBe(false);
  });

  it("shows current work and excludes waiting time from the execution clock", async () => {
    await expanded();
    expect(screen.getByRole("heading", { name: base.title })).toBeVisible();
    expect(screen.getByText(base.activity)).toBeVisible();
    expect(screen.getByText("2m 10s")).toBeVisible();
    snapshot = { items: [{ ...base, status: "waiting", activeSince: null, activity: "Precisa de uma decisão" }], truncated: false };
    act(() => events.get("companion:changed")?.({}));
    await screen.findByText("Precisa de uma decisão");
    expect(screen.getByText("2m 00s")).toBeVisible();
    expect(screen.getByRole("button", { name: "Recolher assistente Jarvis" }).querySelector("svg[data-state=waiting]")).toBeInTheDocument();
  });

  it("refreshes during a continuous event stream instead of waiting for the stream to end", async () => {
    snapshot.items[0].activeSince = null;
    await expanded();
    call.mockClear(); vi.useFakeTimers();
    snapshot.items[0].activity = "Atualização durante a execução";
    const stream = window.setInterval(() => events.get("companion:changed")?.({}), 50);
    await act(async () => { await vi.advanceTimersByTimeAsync(350); });
    expect(call.mock.calls.filter(([command]) => command === "get_companion_snapshot").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("Atualização durante a execução")).toBeVisible();
    window.clearInterval(stream);
  });

  it("focuses the activity selector only after clicking it, preserves labels and opens the owning conversation explicitly", async () => {
    snapshot.items.push({ ...base, agentId: "designer-1", role: "designer", title: "Revisar os componentes", activity: "Conferindo espaçamentos" });
    const user = userEvent.setup(); render(<Companion />);
    await user.click(await screen.findByRole("button", { name: /Abrir assistente Jarvis, 2 atividade/ }));
    expect(call).not.toHaveBeenCalledWith("companion_set_interacting", { active: true });
    const selector = await screen.findByRole("combobox", { name: "Conversa ou agente" });
    const icon = selector.querySelector("svg");
    if (!icon) throw new Error("Missing selector icon");
    await user.click(icon);
    expect(call).toHaveBeenCalledWith("companion_set_interacting", { active: true });
    await user.click(await screen.findByRole("option", { name: /Revisar os componentes.*Designer.*subagente/ }));
    expect(screen.getByRole("combobox", { name: "Conversa ou agente" })).toHaveTextContent("Revisar os componentes");
    expect(screen.queryByText("conversation-1/designer-1")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Revisar os componentes" })).toBeVisible();
    expect(screen.getByText("Conferindo espaçamentos")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Abrir conversa no Jarvis" }));
    expect(call).toHaveBeenCalledWith("companion_open_conversation", { conversationId: base.conversationId });
  });

  it("keeps the active detail visible with many archived activities and displays labels rather than IDs", async () => {
    snapshot.items = Array.from({ length: 31 }, (_, index) => ({ ...base, conversationId: `archive-${index}`, status: "completed", activeSince: null, updatedAt: index, title: `Conversa anterior ${index}` }));
    snapshot.items.push({ ...base, conversationId: "current", title: "Implementação atual" });
    await expanded();
    expect(screen.getByRole("heading", { name: "Implementação atual" })).toBeVisible();
    expect(screen.getByRole("combobox", { name: "Conversa ou agente" })).toHaveTextContent("Implementação atual");
    expect(screen.queryByText("current/root")).not.toBeInTheDocument();
    expect(screen.queryByText("Conversa anterior 30")).not.toBeInTheDocument();
  });

  it("prioritizes new work over old failures and keeps the most recent terminal result", async () => {
    snapshot.items = [{ ...base, status: "failed", activeSince: null, updatedAt: 100, title: "Falha antiga" }, { ...base, conversationId: "new-conversation", updatedAt: 1_000_000, title: "Trabalho novo" }];
    await expanded();
    expect(screen.getByRole("heading", { name: "Trabalho novo" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Recolher assistente Jarvis" }).querySelector("svg[data-state=running]")).toBeInTheDocument();
    snapshot.items[1] = { ...snapshot.items[1], status: "completed", activeSince: null, updatedAt: 1_010_000 };
    act(() => events.get("companion:changed")?.({}));
    await waitFor(() => expect(screen.getByRole("button", { name: "Recolher assistente Jarvis" }).querySelector("svg[data-state=completed]")).toBeInTheDocument());
    expect(screen.getByRole("heading", { name: "Trabalho novo" })).toBeVisible();
  });

  it("reuses interactive questions with the real child ID and only takes focus after user interaction", async () => {
    const user = await expanded();
    snapshot = { items: [{ ...base, agentId: "planner-1", status: "waiting", activeSince: null, pendingQuestion: {
      turnId: "turn-1", toolId: "question-1", deadlineAt: 1_030_000,
      questions: [{ id: "scope", question: "Qual escopo usar?", options: [{ label: "Completo", recommended: true }, { label: "Somente visual" }] }],
    } }], truncated: false };
    act(() => events.get("companion:changed")?.({}));
    await screen.findByRole("region", { name: "Perguntas do Jarvis" });
    expect(call).not.toHaveBeenCalledWith("companion_set_interacting", { active: true });
    expect(document.activeElement).not.toHaveTextContent("Qual escopo usar?");
    await user.click(screen.getByRole("button", { name: /Somente visual/ }));
    expect(call).toHaveBeenCalledWith("companion_pause_question", { conversationId: base.conversationId, agentId: "planner-1", turnId: "turn-1", toolId: "question-1" });
    await user.click(screen.getByRole("textbox", { name: "Sua resposta" }));
    expect(call).toHaveBeenCalledWith("companion_set_interacting", { active: true });
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(call).toHaveBeenCalledWith("companion_answer_question", {
      conversationId: base.conversationId, agentId: "planner-1", turnId: "turn-1", toolId: "question-1",
      response: { cancelled: false, answers: [{ id: "scope", value: "Somente visual", selectedLabel: "Somente visual" }] },
    });
  });

  it("keeps failed answers editable and directs other decisions back to the main app", async () => {
    const user = await expanded();
    snapshot.items = [{ ...base, status: "waiting", activeSince: null, pendingQuestion: { turnId: "turn-1", toolId: "question-1", questions: [{ id: "name", question: "Qual o nome?", options: [] }] } }];
    act(() => events.get("companion:changed")?.({}));
    await user.type(await screen.findByRole("textbox", { name: "Sua resposta" }), "Portal");
    const original = call.getMockImplementation();
    let rejected = false;
    call.mockImplementation(async (command, args) => {
      if (command === "companion_answer_question" && !rejected) { rejected = true; throw "A pergunta não está mais disponível."; }
      return original?.(command, args);
    });
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A pergunta não está mais disponível");
    expect(screen.getByRole("textbox", { name: "Sua resposta" })).toHaveValue("Portal");
    expect(screen.getByRole("button", { name: "Enviar respostas" })).toBeEnabled();
    snapshot.items = [{ ...base, status: "waiting", activeSince: null, requiresConversation: true }];
    act(() => events.get("companion:changed")?.({}));
    await user.click(await screen.findByRole("button", { name: "Continuar no Jarvis" }));
    expect(call).toHaveBeenCalledWith("companion_open_conversation", { conversationId: base.conversationId });
    expect(screen.queryByRole("textbox", { name: "Sua resposta" })).not.toBeInTheDocument();
  });

  it("opens a dedicated question from a compact waiting notice without tabs, composer or model controls", async () => {
    snapshot.items = [{ ...base, agentId: "designer-2", status: "waiting", activeSince: null, pendingQuestion: {
      turnId: "turn-q", toolId: "ask-q", questions: [{ id: "scope", question: "Qual tela revisar?", options: [{ label: "Relatório", description: "Revisar a tela atual." }] }],
    } }];
    const user = userEvent.setup(); render(<Companion />);
    expect(await screen.findByRole("status", { name: "Aviso do Jarvito" })).toHaveTextContent("Qual tela revisar?");
    expect(call).toHaveBeenCalledWith("set_companion_bubble", { visible: true });
    await user.click(screen.getByRole("button", { name: "Responder pergunta" }));
    expect(await screen.findByRole("heading", { name: "Qual tela revisar?" })).toBeVisible();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Mensagem para Jarvito" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Modelo do Jarvito" })).not.toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
    expect(screen.queryByText("Seu Jarvis, por perto.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Relatório.*Revisar a tela atual/ }));
    await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
    expect(call).toHaveBeenCalledWith("companion_answer_question", {
      conversationId: base.conversationId, agentId: "designer-2", turnId: "turn-q", toolId: "ask-q", response: { cancelled: false, answers: [{ id: "scope", value: "Relatório", selectedLabel: "Relatório" }] },
    });
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
  });

  it("expires speech after 30 seconds without acknowledging an unseen failed activity", async () => {
    snapshot.items = [{ ...base, status: "failed", activeSince: null, attentionId: "turn-1/failed" }];
    vi.useFakeTimers();
    render(<Companion />);
    await act(async () => { await Promise.resolve(); });
    const speech = screen.getByRole("status", { name: "Aviso do Jarvito" });
    expect(speech).toHaveTextContent("Não consegui concluir esta solicitação.");
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Abrir assistente Jarvis/ }).querySelector("svg[data-state=failed]")).toBeInTheDocument();
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
  });

  it("acknowledges only the terminal activity actually accessed, keeping history and other unseen attention", async () => {
    const first = { ...base, status: "completed" as const, activeSince: null, attentionId: "first/completed", updatedAt: 200 };
    const second = { ...base, conversationId: "other-chat", title: "Outro relatório", status: "failed" as const, activeSince: null, attentionId: "other/failed", updatedAt: 100 };
    snapshot.items = [first, second];
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "ack_companion_item") {
        const id = (args as { attentionId: string }).attentionId;
        snapshot = { ...snapshot, items: snapshot.items.map(item => item.attentionId === id ? { ...item, acknowledged: true } : item) };
        return snapshot;
      }
      return original?.(command, args);
    });
    const user = userEvent.setup(); render(<Companion />);
    await user.click(await screen.findByRole("button", { name: /Abrir assistente Jarvis/ }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: base.conversationId, agentId: null, attentionId: "first/completed" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Recolher assistente Jarvis" }).querySelector("svg[data-state=failed]")).toBeInTheDocument());
    expect(screen.getByRole("heading", { name: base.title })).toBeVisible();
    expect(screen.getByText("Concluído")).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("ack_companion_item", expect.objectContaining({ attentionId: "other/failed" }));
    await user.click(screen.getByRole("combobox", { name: "Conversa ou agente" }));
    await user.click(await screen.findByRole("option", { name: /Outro relatório/ }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: "other-chat", agentId: null, attentionId: "other/failed" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Recolher assistente Jarvis" }).querySelector("svg[data-state=idle]")).toBeInTheDocument());
    expect(screen.getByRole("heading", { name: "Outro relatório" })).toBeVisible();
    expect(screen.getByText("Falhou")).toBeVisible();
  });

  it("communicates the actual response in a global chat instead of announcing the pet's name", async () => {
    snapshot.items = [{ ...base, title: "Jarvito", projectName: "Chat geral", status: "completed", activeSince: null, result: "O próximo feriado será em 12 de outubro." }];
    render(<Companion />);
    const speech = await screen.findByRole("status", { name: "Aviso do Jarvito" });
    expect(speech).toHaveTextContent("Sua resposta está pronta.");
    await waitFor(() => expect(speech).toHaveTextContent("O próximo feriado será em 12 de outubro."));
    expect(speech).not.toHaveTextContent("Concluí: Jarvito");
  });

  it.each(["waiting", "completed", "failed"] as const)("restores a %s bubble only after dragging ends, without acknowledging or dismissing it", async status => {
    snapshot.items = [{ ...base, status, activeSince: null, pendingQuestion: status === "waiting" ? { turnId: "turn-q", toolId: "ask-q", questions: [{ id: "scope", question: "Qual tela revisar?", options: [] }] } : null }];
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "companion_start_drag") {
        events.get("companion:geometry")?.({ expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 96, height: 112 });
        return true;
      }
      return original?.(command, args);
    });
    render(<Companion />);
    await screen.findByRole("status", { name: "Aviso do Jarvito" });
    const pet = screen.getByRole("button", { name: /Abrir assistente Jarvis/ });
    fireEvent.pointerDown(pet, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(pet, { clientX: 22, clientY: 10, buttons: 1 });
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
    const bubbleRequests = call.mock.calls.filter(([command]) => command === "set_companion_bubble").length;
    fireEvent.pointerUp(pet);
    expect(call.mock.calls.filter(([command]) => command === "set_companion_bubble")).toHaveLength(bubbleRequests);
    act(() => events.get("companion:drag-end")?.(null));
    await screen.findByRole("status", { name: "Aviso do Jarvito" });
    expect(call.mock.calls.filter(([command]) => command === "set_companion_bubble")).toHaveLength(bubbleRequests + 1);
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item" || command === "companion_answer_question")).toBe(false);
  });

  it("ignores a bubble response that arrives after native drag geometry", async () => {
    snapshot.items = [{ ...base, status: "completed", activeSince: null }];
    let resolveBubble: ((value: unknown) => void) | undefined;
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "set_companion_bubble") return new Promise(resolve => { resolveBubble = resolve; });
      return original?.(command, args);
    });
    render(<Companion />);
    await waitFor(() => expect(resolveBubble).toBeDefined());
    act(() => events.get("companion:geometry")?.({ expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 96, height: 112 }));
    await act(async () => resolveBubble?.({ expanded: false, bubble: true, robotSide: "right", robotVertical: "bottom", width: 360, height: 300 }));
    const main = screen.getByRole("button", { name: /Abrir assistente Jarvis/ }).closest("main");
    expect(main).toHaveAttribute("data-robot-side", "left");
    expect(main).toHaveAttribute("data-robot-vertical", "top");
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
  });

  it("animates outside-click collapse before shrinking the native island", async () => {
    await expanded();
    vi.useFakeTimers(); call.mockClear();
    act(() => events.get("companion:collapse-request")?.(null));
    const main = screen.getByRole("button", { name: "Recolher assistente Jarvis" }).closest("main");
    expect(main).toHaveAttribute("data-closing", "true");
    expect(screen.getByRole("region", { name: "Assistente Jarvis" })).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("set_companion_expanded", { expanded: false });
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: false });
    expect(screen.queryByRole("region", { name: "Assistente Jarvis" })).not.toBeInTheDocument();
    expect(main).toHaveAttribute("data-closing", "false");
  });

  it("honors an outside-click collapse requested while opening is still in flight", async () => {
    let finishOpen: ((value: unknown) => void) | undefined;
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "set_companion_expanded" && (args as { expanded: boolean }).expanded) return new Promise(resolve => { finishOpen = resolve; });
      return original?.(command, args);
    });
    render(<Companion />);
    fireEvent.click(await screen.findByRole("button", { name: /Abrir assistente Jarvis/ }));
    await waitFor(() => expect(finishOpen).toBeDefined());
    vi.useFakeTimers();
    const geometry = { expanded: true, bubble: false, robotSide: "left", robotVertical: "top", width: 460, height: 600 };
    act(() => {
      events.get("companion:geometry")?.(geometry);
      events.get("companion:collapse-request")?.(null);
    });
    await act(async () => finishOpen?.(geometry));
    expect(screen.getByRole("button", { name: "Recolher assistente Jarvis" }).closest("main")).toHaveAttribute("data-closing", "true");
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(screen.queryByRole("region", { name: "Assistente Jarvis" })).not.toBeInTheDocument();
    expect(vi.mocked(listen).mock.calls.filter(([name]) => name === "companion:collapse-request")).toHaveLength(1);
  });

  it("does not start a drag after a mouse release outside the pet", async () => {
    render(<Companion />);
    const pet = await screen.findByRole("button", { name: /Abrir assistente Jarvis/ });
    fireEvent.pointerDown(pet, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(pet, { clientX: 40, clientY: 10, buttons: 0 });
    fireEvent.pointerMove(pet, { clientX: 60, clientY: 10, buttons: 1 });
    expect(call.mock.calls.some(([command]) => command === "companion_start_drag")).toBe(false);
  });

  it("reads cached usage on opening the tab, follows cache changes and offers manual refresh", async () => {
    const user = await expanded();
    expect(call.mock.calls.some(([command]) => command === "get_companion_usage")).toBe(false);
    await user.click(screen.getByRole("tab", { name: "Limites" }));
    expect(await screen.findByText("72% restante")).toBeVisible();
    expect(screen.getByRole("progressbar", { name: "Codex 5h restante" })).toHaveAttribute("aria-valuenow", "72");
    const count = () => call.mock.calls.filter(([command]) => command === "get_companion_usage").length;
    expect(count()).toBe(1);
    act(() => events.get("companion:usage")?.({}));
    await waitFor(() => expect(count()).toBe(2));
    expect(call).toHaveBeenLastCalledWith("get_companion_usage", undefined);
    await user.click(screen.getByRole("button", { name: "Atualizar limites" }));
    await waitFor(() => expect(count()).toBe(3));
    expect(call).toHaveBeenLastCalledWith("get_companion_usage", { refresh: true });
    const panel = screen.getByRole("tabpanel", { name: "Limites" });
    expect(within(panel).getByText("pessoal")).toBeVisible();
  });

  it("distinguishes dragging from clicking and follows geometry without switching the app", async () => {
    render(<Companion />);
    const pet = await screen.findByRole("button", { name: /Abrir assistente Jarvis, 1 atividade/ });
    fireEvent.pointerDown(pet, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(pet, { clientX: 22, clientY: 10, buttons: 1 });
    fireEvent.pointerUp(pet, { clientX: 22, clientY: 10 });
    fireEvent.click(pet, { detail: 1 });
    expect(call).toHaveBeenCalledWith("companion_start_drag", { robotX: 0, robotY: 0 });
    expect(call).not.toHaveBeenCalledWith("set_companion_expanded", { expanded: true });
    act(() => events.get("companion:geometry")?.({ expanded: true, bubble: false, robotSide: "left", robotVertical: "top", width: 420, height: 520 }));
    const panel = await screen.findByRole("region", { name: "Assistente Jarvis" });
    const movedPet = screen.getByRole("button", { name: "Recolher assistente Jarvis" });
    expect(movedPet.closest(".companion-island")).toContainElement(panel);
    expect(screen.getByRole("tab", { name: "Chat" })).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("companion_open_conversation", expect.anything());
    fireEvent.keyDown(panel, { key: "Escape" });
    await waitFor(() => expect(screen.getByRole("button", { name: /Abrir assistente Jarvis/ })).toHaveAttribute("aria-expanded", "false"));
  });

  it("keeps controls usable when native placement provides a smaller panel", async () => {
    const user = await expanded();
    act(() => events.get("companion:geometry")?.({ expanded: true, bubble: false, robotSide: "left", robotVertical: "bottom", width: 300, height: 320 }));
    expect(screen.getByRole("heading", { name: base.title })).toBeVisible();
    expect(screen.getByRole("button", { name: "Abrir conversa no Jarvis" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Recolher painel" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Assistente Jarvis" })).not.toBeInTheDocument());
  });

  it("keeps the robot inside the island and follows the pointer without requesting focus", async () => {
    await expanded();
    const pet = screen.getByRole("button", { name: "Recolher assistente Jarvis" });
    const main = pet.closest("main");
    if (!main) throw new Error("Missing companion surface");
    fireEvent.pointerMove(main, { clientX: 100, clientY: 70, buttons: 0 });
    expect(pet.style.getPropertyValue("--robot-look-x")).not.toBe("0px");
    expect(pet.querySelector("svg[data-state]")?.getAttribute("data-look-x")).not.toBe("0");
    fireEvent.pointerEnter(pet);
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-hovered", "true");
    expect(pet.closest(".companion-island")).toContainElement(screen.getByRole("region", { name: "Assistente Jarvis" }));
    expect(call).not.toHaveBeenCalledWith("companion_set_interacting", { active: true });
    fireEvent.pointerLeave(main);
    expect(pet.style.getPropertyValue("--robot-look-x")).toBe("0px");
    expect(pet.style.getPropertyValue("--robot-look-y")).toBe("0px");
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-look-x", "0");
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-hovered", "false");
  });

  it("pauses living motions while hidden without restarting any activity", async () => {
    render(<Companion />);
    const pet = await screen.findByRole("button", { name: /Abrir assistente Jarvis/ });
    vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    fireEvent(document, new Event("visibilitychange"));
    expect(pet.closest("main")).toHaveAttribute("data-visible", "false");
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-visible", "false");
    expect(call).not.toHaveBeenCalledWith("companion_open_conversation", expect.anything());
  });

  it("retries a failed readonly load and disposes all subscriptions", async () => {
    const original = call.getMockImplementation();
    let failed = false;
    call.mockImplementation(async (command, args) => {
      if (command === "get_companion_snapshot" && !failed) { failed = true; throw new Error("Falha temporária"); }
      return original?.(command, args);
    });
    const user = userEvent.setup(); const view = render(<Companion />);
    await screen.findByRole("button", { name: /Abrir assistente Jarvis/ });
    await user.click(screen.getByRole("button", { name: /Abrir assistente Jarvis/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Falha temporária");
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    await screen.findByText(base.activity);
    view.unmount();
    await waitFor(() => expect(stop.mock.calls.length).toBeGreaterThanOrEqual(4));
    expect(call).toHaveBeenCalledWith("companion_set_interacting", { active: false });
  });
});
