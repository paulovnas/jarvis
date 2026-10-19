import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { populatedLibrary } from "@/test/library-fixtures";
import { useChat } from "@/hooks/use-chat";
import { ChatArea } from "./ChatArea";
import type { AgentTurn, HistoryPage } from "@/core/chat";
import type { LearningSnapshot } from "@/core/project-learning";
import { clearChatStore, updateChatSnapshot } from "@/core/chat-store";
import { Transcript, TurnBody, type LatestVisibility } from "./Transcript";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);

it("shows routed workflow stages instead of a provider connection and preserves the final delivery", async () => {
  const stage = { ...savedTurn().steps[0], summary: "", tools: [], text: "Etapa 1 · RPG/Planejador em execução." };
  const turn = { ...savedTurn(), status: "running" as const, options: { ...savedTurn().options, workflow: "custom" as const, customWorkflowId: "rpg" }, steps: [stage] };
  const { rerender } = render(<TurnBody turn={turn} />);
  expect(screen.getByRole("region", { name: "Progresso do fluxo" })).toHaveTextContent("RPG/Planejador");
  expect(screen.queryByText("Conectando ao provedor…")).not.toBeInTheDocument();
  const routed = { ...turn, steps: [stage, { ...stage, text: "Etapa 2 · RPG/Artista Técnico 3D em execução." }] };
  rerender(<TurnBody turn={routed} />);
  expect(screen.getByRole("region", { name: "Progresso do fluxo" })).toHaveTextContent("RPG/Artista Técnico 3D");
  rerender(<TurnBody turn={{ ...routed, status: "completed", steps: [...routed.steps, { ...stage, text: "Modelo 3D criado e validado." }] }} />);
  expect(await screen.findByText("Modelo 3D criado e validado.")).toBeVisible();
  expect(screen.queryByText(routed.steps[1].text)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /Trabalhou por/ })).toBeVisible();
});

it.each(["cancelled", "interrupted", "error"] as const)("preserves the last workflow stage as incomplete for a %s turn without an error object", async status => {
  const stage = { ...savedTurn().steps[0], summary: "", tools: [], text: "Etapa 1 · RPG/Planejador em execução." };
  const turn = { ...savedTurn(), status, error: null, steps: [stage, { ...stage, text: "Etapa 2 · RPG/Diretor em execução." }] };
  render(<TurnBody turn={turn} />);
  await userEvent.setup().click(screen.getByRole("button", { name: /Trabalhou por/ }));
  const disclosure = within(screen.getByRole("region", { name: "Progresso do fluxo" })).getByRole("button", { name: /Etapa 2.*RPG\/Diretor/ });
  expect(disclosure).toHaveTextContent("Não concluída");
  expect(screen.queryByText(turn.steps[1].text)).not.toBeInTheDocument();
});

function page(start: number, id = "c1"): HistoryPage {
  return { conversationId: id, history: { start, total: 100 }, compactions: [], navigation: [0, 10, 20, 30, 40, 50, 60, 70, 80, 99].map(index => ({ id: `t${index}`, index, createdAt: 1, user: `Pedido ${index}`, assistant: `Resumo ${index}` })), turns: Array.from({ length: 20 }, (_, i) => ({ ...savedTurn(), id: `t${start + i}`, user: `Pedido ${start + i}`, steps: [{ ...savedTurn().steps[0], text: `Resposta ${start + i}`, summary: "", tools: [] }] })) };
}
function Harness({ id = "c1", onLatestVisibility }: { id?: string; onLatestVisibility?: LatestVisibility }) {
  const library = populatedLibrary(); library.conversations[0].id = id; library.selection.conversationId = id;
  return <ChatArea onLatestVisibility={onLatestVisibility} library={library} chat={useChat(id)} modelGroups={[{ provider: "Codex", models: [{ value: "openai-codex-pessoal/model", label: "Modelo", reasoningLevels: ["medium"], defaultReasoningLevel: "medium" }] }]} />;
}

function measuredViewport() {
  const descriptors = ["clientHeight", "scrollHeight", "scrollTop", "getBoundingClientRect"].map(key => [key, Object.getOwnPropertyDescriptor(Element.prototype, key)] as const);
  const heights = new Map<string, number>();
  const offsets = new WeakMap<Element, number>();
  const observers = new Set<ResizeObserver>();
  const observed = new Map<ResizeObserver, Set<Element>>();
  const callbacks = new Map<ResizeObserver, ResizeObserverCallback>();
  class MeasuredResizeObserver implements ResizeObserver {
    constructor(callback: ResizeObserverCallback) { observers.add(this); observed.set(this, new Set()); callbacks.set(this, callback); }
    observe(element: Element) { observed.get(this)?.add(element); }
    unobserve(element: Element) { observed.get(this)?.delete(element); }
    disconnect() { observers.delete(this); observed.delete(this); callbacks.delete(this); }
  }
  const originalObserver = window.ResizeObserver;
  window.ResizeObserver = MeasuredResizeObserver;
  const originalHeight = Object.getOwnPropertyDescriptor(Element.prototype, "scrollHeight")?.get;
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.dataset.slot === "scroll-area-viewport" ? 600 : 0;
  });
  vi.spyOn(Element.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.slot !== "scroll-area-viewport") return originalHeight?.call(this) as number ?? 0;
    const list = this.querySelector<HTMLElement>("[data-virtual-turns]");
    return Number.parseFloat(list?.style.height ?? "0") + (this.querySelector('[data-history-older]') ? 48 : 0) + (this.querySelector('[data-history-newer]') ? 48 : 0);
  });
  vi.spyOn(Element.prototype, "scrollTop", "get").mockImplementation(function (this: Element) { return offsets.get(this) ?? 0; });
  vi.spyOn(Element.prototype, "scrollTop", "set").mockImplementation(function (this: Element, value: number) { offsets.set(this, Math.max(0, Math.min(value, this.scrollHeight - this.clientHeight))); });
  const originalRect = Element.prototype.getBoundingClientRect;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const viewport = this.closest<HTMLElement>('[data-slot="scroll-area-viewport"]');
    if (this.dataset.slot === "scroll-area-viewport") return new DOMRect(0, 0, 800, 600);
    if (viewport && (this.dataset.turnId || this.hasAttribute("data-virtual-turns"))) {
      const prefix = viewport.querySelector('[data-history-older]') ? 48 : 0;
      const top = prefix - viewport.scrollTop + (this.dataset.turnId ? Number.parseFloat(this.style.top || "0") : 0);
      const height = this.dataset.turnId ? heights.get(this.dataset.turnId) ?? 280 : Number.parseFloat(this.style.height || "0");
      return new DOMRect(0, top, 800, height);
    }
    return originalRect.call(this);
  });
  return {
    heights,
    restore() { window.ResizeObserver = originalObserver; vi.restoreAllMocks(); for (const [key, descriptor] of descriptors) if (descriptor) Object.defineProperty(Element.prototype, key, descriptor); },
    resize(element: HTMLElement, height: number) {
      if (element.dataset.turnId) heights.set(element.dataset.turnId, height);
      const box = { blockSize: height, inlineSize: 800 };
      const entry: ResizeObserverEntry = { target: element, contentRect: new DOMRect(0, 0, 800, height), borderBoxSize: [box], contentBoxSize: [box], devicePixelContentBoxSize: [box] };
      act(() => { for (const observer of observers) if (observed.get(observer)?.has(element)) callbacks.get(observer)?.([entry], observer); });
    },
  };
}

function longConversation() {
  return { ...emptyChat(), history: { start: 0, total: 100 }, navigation: page(0).navigation, turns: Array.from({ length: 100 }, (_, index) => ({ ...savedTurn(), id: `t${index}`, user: `Pedido ${index}`, steps: [{ ...savedTurn().steps[0], text: `Resposta ${index}`, summary: `Observação ${index}`, tools: [] }] })) };
}

function PendingHarness({ pending }: { pending: AgentTurn }) {
  const chat = useChat("c1");
  return chat.snapshot && <Transcript snapshot={chat.snapshot} chat={{ ...chat, pending: true, pendingTurn: pending }} />;
}

describe("measured transcript window", () => {
  beforeEach(() => { clearChatStore(); call.mockReset(); vi.mocked(listen).mockResolvedValue(() => {}); });
  it("bounds mounted turns to the measured viewport with overscan while scrolling", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => geometry.restore());
    call.mockResolvedValue(longConversation());
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    expect(container.querySelectorAll("[data-turn-id]").length).toBeLessThanOrEqual(10);
    expect(screen.queryByText("Resposta 0")).not.toBeInTheDocument();
    viewport.scrollTop = 11_240; fireEvent.scroll(viewport);
    expect(await screen.findByText("Resposta 40")).toBeVisible();
    expect(container.querySelector('[data-turn-id="t38"]')).not.toBeNull();
    expect(container.querySelector('[data-turn-id="t44"]')).not.toBeNull();
    expect(container.querySelector('[data-turn-id="t37"]')).toBeNull();
    expect(container.querySelector('[data-turn-id="t45"]')).toBeNull();
    expect(screen.getByRole("button", { name: "Ir para interação 41: Pedido 40" })).toHaveAttribute("aria-current", "location");
  });
  it("keeps the reading turn at the same offset when a measured turn above it grows", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => geometry.restore());
    call.mockResolvedValue(longConversation());
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    viewport.scrollTop = 11_240; fireEvent.scroll(viewport);
    const reading = container.querySelector<HTMLElement>('[data-turn-id="t40"]')!;
    expect(reading.getBoundingClientRect().top).toBe(-40);
    geometry.resize(container.querySelector<HTMLElement>('[data-turn-id="t38"]')!, 560);
    await waitFor(() => expect(viewport.scrollTop).toBe(11_520));
    expect(reading.getBoundingClientRect().top).toBe(-40);
    expect(container.querySelector('[data-turn-id="t40"]')).toBe(reading);
    geometry.resize(container.querySelector<HTMLElement>('[data-turn-id="t38"]')!, 560);
    expect(viewport.scrollTop).toBe(11_520);
  });
  it("retains focused, expanded and selected turns outside the viewport until interaction ends", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => { document.getSelection()?.removeAllRanges(); geometry.restore(); });
    call.mockResolvedValue(longConversation());
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    viewport.scrollTop = 11_240; fireEvent.scroll(viewport);
    const selected = container.querySelector<HTMLElement>('[data-turn-id="t40"]')!;
    const focused = container.querySelector<HTMLElement>('[data-turn-id="t41"]')!;
    const expanded = container.querySelector<HTMLElement>('[data-turn-id="t42"]')!;
    const disclosure = within(expanded).getByRole("button", { name: /Trabalhou por/ });
    await userEvent.setup().click(disclosure);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    const selection = document.getSelection()!;
    const selectedText = document.createRange();
    selectedText.selectNodeContents(await within(selected).findByText("Pedido 40"));
    const copy = within(focused).getByRole("button", { name: "Copiar resposta" });
    act(() => copy.focus());
    act(() => { selection.removeAllRanges(); selection.addRange(selectedText); document.dispatchEvent(new Event("selectionchange")); });
    expect(selection.toString()).toBe("Pedido 40");
    viewport.scrollTop = 1_440; fireEvent.scroll(viewport);
    expect(container.querySelector('[data-turn-id="t40"]')).toBe(selected);
    expect(container.querySelector('[data-turn-id="t41"]')).toBe(focused);
    expect(container.querySelector('[data-turn-id="t42"]')).toBe(expanded);
    expect(document.activeElement).toBe(copy);
    expect(selection.toString()).toBe("Pedido 40");
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    act(() => { copy.blur(); selection.removeAllRanges(); document.dispatchEvent(new Event("selectionchange")); });
    fireEvent.click(disclosure);
    await waitFor(() => expect(container.querySelector('[data-turn-id="t42"]')).toBeNull());
    expect(container.querySelector('[data-turn-id="t40"]')).toBeNull();
    expect(container.querySelector('[data-turn-id="t41"]')).toBeNull();
    expect(container.querySelectorAll("[data-turn-id]").length).toBeLessThanOrEqual(10);
  });
  it("retains the streaming turn and follows its growth only while reading the latest messages", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => geometry.restore());
    const snapshot = longConversation();
    snapshot.turns[99] = { ...snapshot.turns[99], status: "running" };
    call.mockResolvedValue({ ...snapshot, activeTurnId: "t99" });
    const onLatestVisibility = vi.fn<LatestVisibility>();
    const { container } = render(<Harness onLatestVisibility={onLatestVisibility} />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    viewport.scrollTop = 11_240; fireEvent.scroll(viewport);
    const live = container.querySelector<HTMLElement>('[data-turn-id="t99"]')!;
    const reading = container.querySelector<HTMLElement>('[data-turn-id="t40"]')!;
    geometry.resize(live, 1_000);
    await waitFor(() => expect(container.querySelector<HTMLElement>("[data-virtual-turns]")!.style.height).toBe("28720px"));
    expect(viewport.scrollTop).toBe(11_240);
    expect(reading.getBoundingClientRect().top).toBe(-40);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", false);
    act(() => updateChatSnapshot("c1", current => current && { ...current, turns: current.turns.map(turn => turn.id === "t99" ? { ...turn, steps: [{ ...turn.steps[0], text: "Texto que chegou durante a leitura." }] } : turn) }));
    await screen.findByText("Texto que chegou durante a leitura.");
    expect(container.querySelector('[data-turn-id="t99"]')).toBe(live);
    viewport.scrollTop = viewport.scrollHeight; fireEvent.scroll(viewport);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", true);
    geometry.resize(live, 1_800);
    await waitFor(() => expect(viewport.scrollTop).toBe(28_920));
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", true);
  });
  it("retains the pending turn without remounting it as its text changes", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => geometry.restore());
    call.mockResolvedValue(longConversation());
    const pending = { ...savedTurn(), id: "pending", user: "Pedido em envio", status: "running" as const };
    const { container, rerender } = render(<PendingHarness pending={pending} />);
    await screen.findByText("Pedido em envio");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    viewport.scrollTop = 11_240; fireEvent.scroll(viewport);
    const row = container.querySelector('[data-turn-id="pending"]');
    expect(row).not.toBeNull();
    expect(row).toHaveAttribute("aria-posinset", "101");
    expect(row).toHaveAttribute("aria-setsize", "101");
    rerender(<PendingHarness pending={{ ...pending, steps: [{ ...pending.steps[0], text: "Aguardando confirmação." }] }} />);
    await screen.findByText("Aguardando confirmação.");
    expect(container.querySelector('[data-turn-id="pending"]')).toBe(row);
    expect(viewport.scrollTop).toBe(11_240);
  });
  it("keeps a keyed reading row in place when an older page is prepended", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => geometry.restore());
    call.mockImplementation(async command => command === "get_chat_history" ? page(60) : { ...emptyChat(), ...page(80) });
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    viewport.scrollTop = 628; fireEvent.scroll(viewport);
    const reading = container.querySelector<HTMLElement>('[data-turn-id="t82"]')!;
    expect(reading.getBoundingClientRect().top).toBe(-20);
    await userEvent.setup().click(screen.getByRole("button", { name: "Mensagens anteriores" }));
    await waitFor(() => expect(viewport.scrollTop).toBe(6_228));
    expect(container.querySelector('[data-turn-id="t82"]')).toBe(reading);
    expect(reading.getBoundingClientRect().top).toBe(-20);
    expect(container.querySelectorAll("[data-turn-id]").length).toBeLessThanOrEqual(10);
    expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", before: 80 });
  });
  it("jumps to an unloaded rail entry and returns to a bounded latest window", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => geometry.restore());
    call.mockImplementation(async (command, args) => command === "get_chat_history" ? page(args && "around" in args ? 0 : 80) : { ...emptyChat(), ...page(80) });
    const user = userEvent.setup();
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    await user.click(screen.getByRole("button", { name: "Ir para interação 1: Pedido 0" }));
    expect(await screen.findByText("Resposta 0")).toBeVisible();
    expect(screen.queryByText("Resposta 99")).not.toBeInTheDocument();
    expect(container.querySelectorAll("[data-turn-id]").length).toBeLessThanOrEqual(10);
    await user.click(screen.getByRole("button", { name: "Voltar ao presente" }));
    expect(await screen.findByText("Resposta 99")).toBeVisible();
    expect(screen.queryByText("Resposta 0")).not.toBeInTheDocument();
    expect(container.querySelectorAll("[data-turn-id]").length).toBeLessThanOrEqual(10);
  });
  it("keeps the reading anchor when newer messages are appended to an older page", async ({ onTestFinished }) => {
    const geometry = measuredViewport();
    onTestFinished(() => geometry.restore());
    call.mockImplementation(async command => command === "get_chat_history" ? page(20) : { ...emptyChat(), ...page(0) });
    const onLatestVisibility = vi.fn<LatestVisibility>();
    const { container } = render(<Harness onLatestVisibility={onLatestVisibility} />);
    await screen.findByText("Resposta 0");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    viewport.scrollTop = 4_492; fireEvent.scroll(viewport);
    const reading = container.querySelector<HTMLElement>('[data-turn-id="t16"]')!;
    expect(reading.getBoundingClientRect().top).toBe(-12);
    await userEvent.setup().click(screen.getByRole("button", { name: "Mensagens seguintes" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", after: 20 }));
    expect(container.querySelector('[data-turn-id="t16"]')).toBe(reading);
    expect(viewport.scrollTop).toBe(4_492);
    expect(reading.getBoundingClientRect().top).toBe(-12);
    expect(container.querySelectorAll("[data-turn-id]").length).toBeLessThanOrEqual(10);
    expect(onLatestVisibility).not.toHaveBeenCalledWith("c1", true);
  });
});

describe("lazy transcript navigation", () => {
  beforeEach(() => { clearChatStore(); call.mockReset(); vi.mocked(listen).mockResolvedValue(() => {}); });
  it("presents saved native Live requests as compact cards without rewriting history", async () => {
    const turn = savedTurn();
    turn.user = `Impeccable Live · ação saved-live\nExecute SOMENTE o pedido atual abaixo como Designer. O pedido e a seleção atuais são:\n${JSON.stringify({ id: "saved-live", type: "generate", freeformPrompt: "Ajuste as cores dos ícones", count: 3 })}`;
    const original = turn.user;
    call.mockResolvedValue({ ...emptyChat(), turns: [turn] });
    render(<Harness />);
    expect(await screen.findByText("Designer Live")).toBeVisible();
    expect(screen.getByText("Ajuste as cores dos ícones")).toBeVisible();
    expect(screen.getByRole("button", { name: "Ver detalhes" })).toBeVisible();
    expect(screen.queryByText(/Execute SOMENTE/)).not.toBeInTheDocument();
    expect(turn.user).toBe(original);
  });
  it("retains the work disclosure when a saved turn contains only native Core activity", async () => {
    const user = userEvent.setup();
    const turn = savedTurn();
    turn.steps = [{ ...turn.steps[0], text: "Pronto.", summary: "", tools: [], coreActivities: [{ component: "ponytail", action: "coding_guidance", status: "applied", summary: "Orientações aplicadas", sources: [], durationMs: 0 }] }];
    render(<TurnBody turn={turn} />);
    await user.click(screen.getByRole("button", { name: /Trabalhou por/ }));
    await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
    expect(screen.getByText("Ponytail")).toBeVisible();
    expect(screen.getByText(/Orientações aplicadas/)).toBeVisible();
  });
  it("shows a background learning result below the correct answer without reopening the chat", async () => {
    let changed: EventCallback<string> | undefined;
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (event === "project:learning-changed") changed = callback as EventCallback<string>;
      return () => {};
    });
    const learned: LearningSnapshot = { enabled: true, revision: 0, pending: 0, notice: null, lessons: [] };
    call.mockImplementation(async command => command === "get_project_learning" ? structuredClone(learned) : { ...emptyChat(), turns: [savedTurn()] });
    const user = userEvent.setup();
    render(<Harness />);
    await screen.findByRole("button", { name: "Copiar resposta" });
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_project_learning", { projectId: populatedLibrary().selection.projectId }));
    expect(screen.queryByRole("button", { name: /Aprendizados registrados/ })).not.toBeInTheDocument();
    learned.lessons.push({ id: "spacing", scope: ".", content: "Revise o espaçamento entre botões e badges.", check: "", topics: ["layout"], status: "active", origin: "feedback", revision: 1, updatedAt: 1, evidence: [{ conversationId: "c1", messageId: "turn1", excerpt: "Sempre revise o espaçamento", createdAt: 1 }] });
    await act(async () => changed?.({ event: "project:learning-changed", id: 1, payload: populatedLibrary().selection.projectId! }));
    const trigger = await screen.findByRole("button", { name: /Aprendizados registrados/ });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await user.click(trigger);
    expect(screen.getByText(learned.lessons[0].content)).toBeVisible();
  });
  it("reports reading only at the latest messages and clears visibility when leaving", async () => {
    call.mockResolvedValue({ ...emptyChat(), ...page(80) });
    const onLatestVisibility = vi.fn<LatestVisibility>();
    const view = render(<Harness onLatestVisibility={onLatestVisibility} />);
    await screen.findByText("Resposta 99");
    const viewport = view.container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    Object.defineProperties(viewport, { scrollHeight: { configurable:true, value:1600 }, clientHeight: { configurable:true, value:600 } });
    viewport.scrollTop = 1000; fireEvent.scroll(viewport);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", true);
    viewport.scrollTop = 400; fireEvent.scroll(viewport);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", false);
    viewport.scrollTop = 1000; fireEvent.scroll(viewport);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", true);
    view.unmount();
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", false);
  });
  it("never treats the end of an older history page as the latest message", async () => {
    call.mockResolvedValue({ ...emptyChat(), ...page(0) });
    const onLatestVisibility = vi.fn<LatestVisibility>();
    const view = render(<Harness onLatestVisibility={onLatestVisibility} />);
    await screen.findByText("Resposta 19");
    const viewport = view.container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    Object.defineProperties(viewport, { scrollHeight: { configurable:true, value:1600 }, clientHeight: { configurable:true, value:600 } });
    viewport.scrollTop = 1000; fireEvent.scroll(viewport);
    expect(onLatestVisibility).not.toHaveBeenCalledWith("c1", true);
  });
  it("oculta a navegação com menos de dez trechos", async () => {
    call.mockResolvedValue({ ...emptyChat(), ...page(80), navigation: page(80).navigation.slice(0, 9) });
    render(<Harness />);
    await screen.findByText("Resposta 99");
    expect(screen.queryByRole("navigation", { name: "Navegar pela conversa" })).not.toBeInTheDocument();
  });
  it("starts the topic rail at the latest entry and preserves browsing until interaction ends", async ({ onTestFinished }) => {
    onTestFinished(() => { vi.restoreAllMocks(); });
    vi.spyOn(Element.prototype, "scrollHeight", "get").mockReturnValue(1200);
    vi.spyOn(Element.prototype, "clientHeight", "get").mockReturnValue(240);
    call.mockResolvedValue({ ...emptyChat(), ...page(80) });
    const user = userEvent.setup();
    render(<Harness />);
    const rail = await screen.findByRole("navigation", { name: "Navegar pela conversa" });
    expect(rail.scrollTop).toBe(960);
    const addTopic = () => act(() => {
      updateChatSnapshot("c1", current => {
        if (!current) return current;
        const entries = current.navigation ?? [];
        const index = (entries[entries.length - 1]?.index ?? -1) + 1;
        return { ...current, navigation: [...entries, { id: `t${index}`, index, createdAt: 1, user: `Pedido ${index}`, assistant: "" }] };
      });
    });
    await user.hover(rail);
    rail.scrollTop = 200;
    fireEvent.scroll(rail);
    addTopic();
    expect(rail.scrollTop).toBe(200);
    const topic = screen.getByRole("button", { name: "Ir para interação 1: Pedido 0" });
    act(() => topic.focus());
    await user.unhover(rail);
    addTopic();
    expect(rail.scrollTop).toBe(200);
    act(() => topic.blur());
    addTopic();
    expect(rail.scrollTop).toBe(960);
  });
  it("ignores elastic overscroll but loads older messages on a real upward scroll", async () => {
    call.mockImplementation(async command => command === "get_chat_history" ? page(60) : { ...emptyChat(), ...page(80) });
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    Object.defineProperties(viewport, { scrollHeight: { configurable: true, value: 1600 }, clientHeight: { configurable: true, value: 600 } });
    viewport.scrollTop = 400; fireEvent.scroll(viewport);
    viewport.scrollTop = -40; fireEvent.scroll(viewport);
    viewport.scrollTop = 1100; fireEvent.scroll(viewport);
    expect(call.mock.calls.filter(([command]) => command === "get_chat_history")).toHaveLength(0);
    viewport.scrollTop = 100; fireEvent.scroll(viewport);
    await screen.findByText("Resposta 60");
    expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", before: 80 });
  });
  it("loads only the initial page, jumps by excerpt, and keeps the composer draft", async () => {
    call.mockImplementation(async (command, args) => {
      if (command === "subscribe_chat") return { ...emptyChat(), ...page(80) };
      if (command === "get_chat_history") return page(args && "around" in args ? 0 : 80);
      return [];
    });
    const user = userEvent.setup(); render(<Harness />);
    expect(await screen.findByText("Pedido 99")).toBeInTheDocument();
    expect(screen.queryByText("Resposta 0")).not.toBeInTheDocument();
    await user.type(await screen.findByRole("textbox"), "Meu rascunho");
    const first = screen.getByRole("button", { name: "Ir para interação 1: Pedido 0" });
    act(() => first.focus());
    expect(await screen.findByText("Resumo 0")).toBeInTheDocument();
    await user.click(first);
    expect(await screen.findByText("Resposta 0")).toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", around: 0 });
    expect(screen.queryByText("Resposta 99")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveTextContent("Meu rascunho");
    await user.click(screen.getByRole("button", { name: "Voltar ao presente" }));
    expect(await screen.findByText("Resposta 99")).toBeInTheDocument();
  }, 15000);
  it("discards a history response after switching conversation", async () => {
    let resolve: (value: unknown) => void = () => {};
    call.mockImplementation(async (command, args) => command === "get_chat_history" ? new Promise(done => { resolve = done; }) : { ...emptyChat(args && "conversationId" in args ? String(args.conversationId) : "c1"), ...page(80, args && "conversationId" in args ? String(args.conversationId) : "c1") });
    const user = userEvent.setup(); const { rerender } = render(<Harness />);
    await user.click(await screen.findByRole("button", { name: "Mensagens anteriores" }));
    expect(screen.getByRole("status", { name: "Carregando trecho" })).toBeInTheDocument();
    rerender(<Harness id="c2" />);
    await waitFor(() => expect(call).toHaveBeenCalledWith("subscribe_chat", { conversationId: "c2" }));
    await act(async () => resolve(page(60)));
    expect(screen.queryByText("Resposta 60")).not.toBeInTheDocument();
    expect(await screen.findByText("Resposta 99")).toBeInTheDocument();
  });

  it("offers retry only on the newest failed turn", async () => {
    const older = {
      ...savedTurn(),
      id: "older-failure",
      status: "error" as const,
      error: { code: "provider_retry_exhausted", message: "Falha anterior." },
    };
    const newest = {
      ...savedTurn(),
      id: "newest-failure",
      status: "error" as const,
      error: { code: "provider_retry_exhausted", message: "Falha mais recente." },
    };
    const stored = {
      ...emptyChat(),
      revision: 3,
      turns: [older, newest],
      history: { start: 0, total: 2 },
    };
    call.mockImplementation(async command => command === "get_chat_agent_models" ? {} : stored);

    render(<Harness />);

    expect(await screen.findByText("Falha anterior.")).toBeInTheDocument();
    expect(await screen.findByText("Falha mais recente.")).toBeInTheDocument();
    expect(await screen.findAllByRole("button", { name: "Tentar novamente" })).toHaveLength(1);
  });
});

describe("estado do turno", () => {
  it("keeps the latest running text as an observation and promotes only the completed text to the final answer", async () => {
    const firstStep = {
      ...savedTurn().steps[0],
      summary: "Analisando o projeto",
      text: "Vou inspecionar os arquivos relevantes.",
      tools: [],
    };
    const running = {
      ...savedTurn(),
      status: "running" as const,
      createdAt: Date.now(),
      steps: [firstStep],
    };
    const { rerender } = render(<TurnBody turn={running} />);

    const observation = await screen.findByText("Vou inspecionar os arquivos relevantes.");
    expect(observation.closest("[data-execution-observation]")).not.toBeNull();
    expect(screen.getByLabelText("Atividades da execução atual")).toBeVisible();
    expect(screen.queryByRole("button", { name: /Trabalhou por/ })).not.toBeInTheDocument();

    rerender(<TurnBody turn={{
      ...running,
      status: "completed",
      durationMs: 4_000,
      steps: [firstStep, { ...firstStep, summary: "", text: "A análise foi concluída.", tools: [] }],
    }} />);

    expect(screen.getByRole("button", { name: /Trabalhou por 4s/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Vou inspecionar os arquivos relevantes.")).not.toBeInTheDocument();
    expect(await screen.findByText("A análise foi concluída.")).toBeVisible();
  });

  it("preserves the delivery report when automatic LSP feedback adds a final acknowledgement", async () => {
    const user = userEvent.setup();
    const turn = savedTurn();
    const step = turn.steps[0];
    const report = "Corrigi o descarte da transferência. Os testes passaram; a homologação continua pendente.";
    const acknowledgement = "O LSP não reportou diagnósticos novos.";
    turn.steps = [
      { ...step, text: "Vou investigar a transferência." },
      { ...step, tools: [], summary: "", text: report, coreActivities: [{ component: "lsp", action: "file_diagnostics", status: "applied", summary: "Sem diagnósticos novos", sources: ["app.ts"], durationMs: 1 }] },
      { ...step, tools: [], summary: "", text: acknowledgement },
    ];
    render(<TurnBody turn={turn} />);

    expect(await screen.findByText(report)).toBeVisible();
    expect(screen.getByText(acknowledgement)).toBeVisible();
    expect(screen.queryByText("Vou investigar a transferência.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Salvar resposta em Markdown" }));
    expect(call).toHaveBeenCalledWith("save_markdown_document", expect.objectContaining({ content: `${report}\n\n${acknowledgement}` }));
    await user.click(screen.getByRole("button", { name: /Trabalhou por/ }));
    expect(await screen.findByText("Vou investigar a transferência.")).toBeVisible();
    expect(screen.getAllByText(report)).toHaveLength(1);
    expect(screen.getAllByText(acknowledgement)).toHaveLength(1);
  });

  it("does not promote a report or LSP acknowledgement preceding a later tool call", async () => {
    const turn = savedTurn();
    const step = turn.steps[0];
    turn.steps = [
      { ...step, tools: [], summary: "", text: "A primeira correção foi concluída.", coreActivities: [{ component: "lsp", action: "file_diagnostics", status: "applied", summary: "Sem diagnósticos novos", sources: ["app.ts"], durationMs: 1 }] },
      { ...step, text: "Vou corrigir também a validação." },
      { ...step, tools: [], summary: "", text: "A correção final inclui a validação." },
    ];
    render(<TurnBody turn={turn} />);
    expect(await screen.findByText("A correção final inclui a validação.")).toBeVisible();
    expect(screen.queryByText("A primeira correção foi concluída.")).not.toBeInTheDocument();
    expect(screen.queryByText("Vou corrigir também a validação.")).not.toBeInTheDocument();
  });

  it("apresenta uma pausa do watchdog como retomável, sem tratá-la como falha", () => {
    render(<TurnBody turn={{
      ...savedTurn(),
      status: "interrupted",
      error: {
        code: "progress_paused",
        message: "O histórico foi preservado. Envie uma nova mensagem para continuar.",
      },
    }} />);

    expect(screen.getByText("Execução pausada")).toBeInTheDocument();
    expect(screen.getByText(/histórico foi preservado/i)).toBeInTheDocument();
  });

  it("shows a superseded Live generation as a neutral choice receipt and retains the work history", async () => {
    const turn = savedTurn();
    turn.status = "cancelled";
    turn.error = { code: "impeccable_live_superseded", message: "Escolha recebida no Live. A geração anterior foi encerrada." };
    render(<TurnBody turn={turn} onRetry={vi.fn()} />);

    expect(await screen.findByText(turn.error.message)).toBeVisible();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByText("Execução interrompida")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Tentar novamente" })).not.toBeInTheDocument();
    const work = screen.getByRole("button", { name: /Trabalhou por/ });
    await userEvent.setup().click(work);
    expect(work).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("list", { name: "Etapas da execução" })).toBeVisible();
  });

  it("retains the interrupted warning for ordinary user cancellation", () => {
    render(<TurnBody turn={{ ...savedTurn(), status: "cancelled", error: { code: "cancelled", message: "Execução interrompida." } }} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Execução interrompida");
  });

  it("shows a blocking failure once when the final text repeats the turn error", () => {
    const message = "A observação da aprovação ainda precisa ser incorporada.";
    const turn = savedTurn();
    turn.status = "error";
    turn.error = { code: "publication_blocked", message };
    turn.steps = [{ ...turn.steps[0], summary: "", tools: [], text: message }];

    render(<TurnBody turn={turn} />);

    expect(screen.getByRole("alert")).toHaveTextContent(message);
    expect(screen.getAllByText(message)).toHaveLength(1);
  });

  it("offers to continue a failed turn from the error message", async () => {
    const retry = vi.fn();
    const turn = savedTurn();
    turn.status = "error";
    turn.options.workflow = "publication";
    turn.error = { code: "provider_retry_exhausted", message: "A conexão com o provedor falhou." };

    render(<TurnBody turn={turn} onRetry={retry} />);

    await userEvent.setup().click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(retry).toHaveBeenCalledOnce();
    expect(retry).toHaveBeenCalledWith(turn.id);
  });

  it("continues a failed canvas workflow through the same retry action", async () => {
    const retry = vi.fn();
    const turn = savedTurn();
    turn.status = "error";
    turn.options.workflow = "custom";
    turn.options.customWorkflowId = "custom-flow";
    turn.error = { code: "workflow_failed", message: "Uma etapa falhou." };

    render(<TurnBody turn={turn} onRetry={retry} />);

    await userEvent.setup().click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(retry).toHaveBeenCalledWith(turn.id);
  });
});
