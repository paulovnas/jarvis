import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CompanionItem, CompanionSnapshot } from "@/core/companion";
import type { AccountUsage } from "@/core/provider-usage";
import { Companion } from "./Companion";
import type { RobotProps } from "./Robot";
import { voiceSession, voiceSettings } from "@/test/voice-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./Robot", () => ({ Robot: ({ status, gesture, visible, hovered, dragging, expanded, walking, lookX, lookY }: RobotProps) =>
  <svg aria-hidden="true" data-state={status} data-gesture={gesture} data-visible={visible} data-hovered={hovered} data-dragging={dragging} data-expanded={expanded} data-walking={walking} data-look-x={lookX} data-look-y={lookY} />,
}));
const call = vi.mocked(invoke);
const events = new Map<string, (payload: unknown) => void>();
const stop = vi.fn();
const base: CompanionItem = {
  conversationId: "conversation-1", agentId: null, projectId: "project-1", projectName: "Portal", global: false,
  title: "Melhorar o relatório", role: "builder", status: "running", activity: "Verificando o relatório",
  durationMs: 120_000, activeSince: 990_000, updatedAt: 1_000_000, requiresConversation: false, attentionId: "turn-1/running", acknowledged: false,
  revision: 1, tasks: [],
};
let snapshot: CompanionSnapshot;
const account: AccountUsage & { providerKind: string } = {
  alias: "openai-codex-pessoal", providerKind: "openai-codex", fetchedAt: 1_000_000, email: null, plan: null, error: null, resetCredits: null,
  windows: [{ id: "window-1", group: "Codex", thirdParty: false, label: "5h", durationSeconds: 18_000, remainingPercent: 72, resetsAt: 1_200_000 }],
};
let accounts: (typeof account)[];
const geometryFor = (expanded: boolean, height = 160, bubble = false) => ({
  expanded, bubble, robotSide: "left", robotVertical: "top", notchWidth: 0, notchHeight: 0, headerHeight: 32, dragAxis: "horizontal",
  width: expanded || bubble ? 640 : 288, height: expanded || bubble ? height : 32,
  compactX: expanded || bubble ? 176 : 0, compactY: 0, compactWidth: 288, compactHeight: 32,
  surfaceX: 0, surfaceY: 0, surfaceWidth: expanded || bubble ? 640 : 288, surfaceHeight: expanded || bubble ? height : 32,
});
async function expanded() {
  const user = userEvent.setup();
  render(<Companion />);
  await screen.findByRole("button", { name: /Abrir assistente Jarvis, 1 atividade/ });
  await user.click(screen.getByRole("button", { name: /Abrir assistente Jarvis/ }));
  await screen.findByRole("region", { name: "Assistente Jarvis" });
  return user;
}
function measureSliders() {
  // Base UI edge-aligned thumbs require measured widths; jsdom has no layout.
  const original = Element.prototype.getBoundingClientRect;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    if (this.matches("[data-base-ui-slider-control]")) return new DOMRect(0, 0, 200, 12);
    if (this.matches("[data-slot=slider-thumb]")) return new DOMRect(0, 0, 12, 12);
    return original.call(this);
  });
}

describe("Desktop companion", () => {
  beforeEach(() => {
    accounts = [account];
    snapshot = { items: [{ ...base }], truncated: false };
    events.clear(); stop.mockClear();
    vi.spyOn(Date, "now").mockReturnValue(1_000_000);
    vi.stubGlobal("PointerEvent", MouseEvent);
    vi.mocked(listen).mockReset().mockImplementation(async (name, callback) => {
      events.set(String(name), payload => callback({ event: String(name), id: 1, payload }));
      return stop;
    });
    let expandedState = false;
    let expandedHeight = 160;
    call.mockReset().mockImplementation(async (command, args) => {
      if (command === "get_companion_snapshot") return snapshot;
      if (command === "get_companion_usage") return accounts;
      if (command === "get_companion_speech_volume") return 1;
      if (command === "set_companion_speech_volume") return (args as { volume: number }).volume;
      if (command === "set_companion_expanded") {
        const isExpanded = (args as { expanded: boolean }).expanded;
        expandedState = isExpanded;
        expandedHeight = (args as { height?: number }).height ?? 160;
        return geometryFor(isExpanded, expandedHeight);
      }
      if (command === "set_companion_bubble") {
        const bubble = (args as { visible: boolean }).visible && !expandedState;
        return geometryFor(expandedState, expandedHeight, bubble);
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

  it("opens the assistant without a phone action or microphone indicator", async () => {
    await expanded();
    expect(screen.queryByRole("button", { name: /Ligar para Jarvito|Encerrar ligação|Ditar mensagem|Concluir ditado/ })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Microfone ativo no Jarvito")).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Chat" })).toBeVisible();
  });

  it("opens an announcement only after audio is ready and keeps it visible throughout speech", async () => {
    const original = call.getMockImplementation();
    const preparing = voiceSession({ id: "notice-audio", target: "companion-notice", owner: "companion", mode: "announcement", phase: "preparing", revision: 100 });
    call.mockImplementation(async (command, args) => {
      if (command === "get_voice_settings") return voiceSettings();
      if (command === "start_voice_session") return preparing;
      return original?.(command, args);
    });
    render(<Companion />);
    await screen.findByRole("button", { name: /Abrir assistente Jarvis, 1 atividade/ });
    await waitFor(() => expect(events.has("voice:state")).toBe(true));
    await act(async () => {});
    snapshot = { ...snapshot, items: [{ ...base, status: "completed", activeSince: null, attentionId: "turn-1/completed" }] };
    act(() => events.get("companion:changed")?.({}));
    await waitFor(() => expect(call).toHaveBeenCalledWith("start_voice_session", expect.objectContaining({ mode: "announcement" })));
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
    expect(call).not.toHaveBeenCalledWith("set_companion_bubble", { visible: true });
    vi.useFakeTimers();
    await act(async () => { await vi.advanceTimersByTimeAsync(45_000); });
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
    await act(async () => { events.get("voice:state")?.({ ...preparing, phase: "speaking", revision: 101 }); });
    expect(screen.getByRole("status", { name: "Aviso do Jarvito" })).toBeInTheDocument();
    await act(async () => { await vi.advanceTimersByTimeAsync(35_000); });
    expect(screen.getByRole("status", { name: "Aviso do Jarvito" })).toBeInTheDocument();
    await act(async () => { events.get("voice:state")?.({ ...preparing, phase: "idle", revision: 102 }); });
    await act(async () => { await vi.advanceTimersByTimeAsync(30_400); });
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
  });

  it("shows task counts and states in the activity card and updates them without opening the main app", async () => {
    snapshot.items[0].tasks = [
      { id: "sync", title: "Completar sincronização automática", status: "in_progress" },
      { id: "screen", title: "Integrar a tela principal", status: "pending" },
      { id: "errors", title: "Sinalizar falhas", status: "blocked" },
      { id: "tests", title: "Validar os critérios", status: "completed" },
    ];
    const user = await expanded();
    const card = screen.getByLabelText("Atividade selecionada");
    const progress = within(card).getByRole("button", { name: "Tarefas: 1 de 4 tarefas concluídas, 1 em andamento, 1 pendente, 1 bloqueada" });
    expect(progress).toHaveTextContent("1/4");
    expect(progress).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("list", { name: "Tarefas do agente" })).not.toBeInTheDocument();
    await user.click(progress);
    expect(progress).toHaveAttribute("aria-expanded", "true");
    const tasks = screen.getByRole("list", { name: "Tarefas do agente" });
    expect(within(tasks).getAllByRole("listitem")).toHaveLength(4);
    for (const text of ["Completar sincronização automática", "Integrar a tela principal", "Sinalizar falhas", "Validar os critérios", "Em andamento", "Pendente", "Bloqueada", "Concluída"]) expect(within(tasks).getByText(text)).toBeVisible();
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 400 }));
    snapshot.items[0] = { ...snapshot.items[0], tasks: snapshot.items[0].tasks.map(task => ({ ...task, status: "completed" })) };
    act(() => events.get("companion:changed")?.({}));
    await waitFor(() => expect(progress).toHaveTextContent("4/4"));
    expect(within(tasks).getAllByText("Concluída")).toHaveLength(4);
    await user.click(progress);
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 160 }));
    expect(screen.queryByRole("list", { name: "Tarefas do agente" })).not.toBeInTheDocument();
    expect(call.mock.calls.some(([command]) => command === "companion_open_conversation")).toBe(false);
  });

  it("does not show a task counter when no plan exists", async () => {
    await expanded();
    expect(screen.queryByRole("button", { name: /^Tarefas:/ })).not.toBeInTheDocument();
    expect(screen.getByText("Trabalhando")).toBeVisible();
  });

  it("sleeps after two idle minutes despite snapshot refreshes and wakes on interaction and new work", async () => {
    snapshot.items = [];
    vi.useFakeTimers();
    render(<Companion />);
    await act(async () => {});
    const pet = screen.getByRole("button", { name: "Abrir assistente Jarvis" });
    expect(call).toHaveBeenCalledWith("get_companion_snapshot");
    const robot = () => pet.querySelector("svg[data-state]");
    await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
    snapshot = { items: [], truncated: false };
    act(() => events.get("companion:changed")?.(null));
    await act(async () => { await vi.advanceTimersByTimeAsync(59_999); });
    expect(robot()).toHaveAttribute("data-gesture", "none");
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(robot()).toHaveAttribute("data-gesture", "sleep");
    fireEvent.pointerEnter(pet);
    expect(robot()).toHaveAttribute("data-gesture", "none");
    await act(async () => { await vi.advanceTimersByTimeAsync(120_000); });
    expect(robot()).toHaveAttribute("data-gesture", "sleep");
    fireEvent.wheel(pet);
    expect(robot()).toHaveAttribute("data-gesture", "none");
    await act(async () => { await vi.advanceTimersByTimeAsync(120_000); });
    fireEvent.keyDown(pet, { key: "ArrowRight" });
    expect(robot()).toHaveAttribute("data-gesture", "none");
    await act(async () => { await vi.advanceTimersByTimeAsync(120_000); });
    snapshot.items = [{ ...base }];
    act(() => events.get("companion:changed")?.(null));
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    expect(robot()).toHaveAttribute("data-state", "running");
    expect(robot()).toHaveAttribute("data-gesture", "none");
    await act(async () => { await vi.advanceTimersByTimeAsync(120_000); });
    expect(robot()).toHaveAttribute("data-gesture", "none");
  });

  it.each(["running", "reconnecting", "waiting"] as const)("stays awake during %s activity", async status => {
    snapshot.items = [{ ...base, status }];
    render(<Companion />);
    const pet = await screen.findByRole("button", { name: /Abrir assistente Jarvis/ });
    await waitFor(() => expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-state", status));
    vi.useFakeTimers();
    await act(async () => { await vi.advanceTimersByTimeAsync(180_000); });
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-gesture", "none");
  });

  it("stays awake while the island is open for conversation or questions", async () => {
    snapshot.items = [];
    render(<Companion />);
    const pet = await screen.findByRole("button", { name: "Abrir assistente Jarvis" });
    fireEvent.click(pet);
    await screen.findByRole("button", { name: "Interagir com Jarvito" });
    vi.useFakeTimers();
    await act(async () => { await vi.advanceTimersByTimeAsync(180_000); });
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-gesture", "none");
  });

  it("suppresses the native context menu across the island, document and portalled controls until unmounted", async () => {
    snapshot.items.push({ ...base, agentId: "designer", role: "designer", title: "Revisar a interface" });
    const user = userEvent.setup();
    const { unmount } = render(<Companion />);
    const rightClick = (element: Element) => {
      const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      fireEvent(element, event);
      return event.defaultPrevented;
    };
    const opener = await screen.findByRole("button", { name: "Abrir ilha do Jarvito" });
    expect(rightClick(opener)).toBe(true);
    expect(rightClick(document.body)).toBe(true);
    await user.click(opener);
    const panel = await screen.findByRole("region", { name: "Assistente Jarvis" });
    expect(rightClick(panel)).toBe(true);
    await user.click(screen.getByRole("combobox", { name: "Conversa ou agente" }));
    const option = await screen.findByRole("option", { name: /Revisar a interface/ });
    expect(panel).not.toContainElement(option);
    expect(rightClick(option)).toBe(true);
    unmount();
    expect(rightClick(document.body)).toBe(false);
  });

  it("indicates ongoing work only while the island is compact and visible", async () => {
    const user = userEvent.setup();
    render(<Companion />);
    const opener = await screen.findByRole("button", { name: "Abrir ilha do Jarvito" });
    const island = opener.closest(".companion-island");
    await waitFor(() => expect(opener).toHaveAccessibleDescription("Jarvis está trabalhando"));
    const indicator = screen.getByLabelText("1 atividades para acompanhar");
    expect(indicator).toHaveAttribute("data-working", "true");
    expect(indicator).toHaveAttribute("data-status", "running");
    expect(indicator).toHaveClass("companion-compact-badge");
    expect(island).toHaveAttribute("data-working", "true");
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    fireEvent(document, new Event("visibilitychange"));
    expect(island).toHaveAttribute("data-working", "false");
    hidden.mockReturnValue(false);
    fireEvent(document, new Event("visibilitychange"));
    expect(island).toHaveAttribute("data-working", "true");
    await user.click(opener);
    await screen.findByRole("region", { name: "Assistente Jarvis" });
    expect(island).toHaveAttribute("data-working", "false");
    expect(screen.getByRole("heading", { name: base.title })).toBeVisible();
  });

  it("keeps the compact work indicator while another conversation waits and clears it when execution stops", async () => {
    snapshot.items.push({ ...base, conversationId: "waiting-chat", status: "waiting", activeSince: null, attentionId: "waiting-chat/waiting" });
    const user = userEvent.setup();
    render(<Companion />);
    const notice = await screen.findByRole("status", { name: "Aviso do Jarvito" });
    const island = notice.closest(".companion-island");
    expect(island).toHaveAttribute("data-working", "false");
    await user.click(screen.getByRole("button", { name: "Dispensar aviso" }));
    const opener = await screen.findByRole("button", { name: "Abrir ilha do Jarvito" });
    expect(opener).toHaveAccessibleDescription("Jarvis está trabalhando");
    expect(island).toHaveAttribute("data-working", "true");
    expect(opener.closest("main")).toHaveAttribute("data-status", "waiting");
    snapshot.items[0] = { ...snapshot.items[0], status: "completed", acknowledged: true, activeSince: null };
    act(() => events.get("companion:changed")?.({}));
    await waitFor(() => expect(island).toHaveAttribute("data-working", "false"));
    expect(opener).not.toHaveAttribute("aria-description");
  });

  it("keeps one living character inside the compact strip and horizontal dashboard", async () => {
    const user = userEvent.setup(); render(<Companion />);
    const pet = await screen.findByRole("button", { name: /Abrir assistente Jarvis, 1 atividade/ });
    const character = pet.querySelector("svg[data-state]");
    expect(screen.getByRole("button", { name: "Abrir ilha do Jarvito" })).toHaveTextContent("");
    expect(screen.getByLabelText("1 atividades para acompanhar")).toHaveTextContent("");
    await user.click(pet);
    await screen.findByRole("region", { name: "Assistente Jarvis" });
    expect(pet.querySelector("svg[data-state]")).toBe(character);
    expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 160 });
    expect(screen.getByRole("tab", { name: "Atividade" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "Chat" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Nova conversa com Jarvito" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Recolher painel" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Ajustes" })).toBeVisible();
    expect(screen.queryByText("Seu Jarvis por perto")).not.toBeInTheDocument();
  });

  it("shows readable sound settings and lets the user toggle the saved preference through its label", async () => {
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => command === "set_companion_sound"
      ? (args as { enabled: boolean }).enabled : original?.(command, args));
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Ajustes" }));
    const panel = screen.getByRole("tabpanel", { name: "Ajustes" });
    expect(within(panel).getByRole("heading", { name: "Ajustes do Jarvito" })).toBeVisible();
    expect(within(panel).getByText("Abertura, perguntas, atividades e conclusões.")).toBeVisible();
    expect(within(panel).getByRole("switch", { name: "Sons de interação" })).toBeChecked();
    await user.click(within(panel).getByText("Sons de interação"));
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_sound", { enabled: false }));
    expect(within(panel).getByRole("switch", { name: "Sons de interação" })).not.toBeChecked();
  });

  it("offers independent sound and speech mute controls in the header popover", async () => {
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => command === "set_companion_sound" || command === "set_companion_speech"
      ? (args as { enabled: boolean }).enabled : original?.(command, args));
    const user = await expanded();
    await user.click(screen.getByRole("button", { name: "Sons e fala do Jarvito" }));
    const sound = await screen.findByRole("switch", { name: "Silenciar sons" });
    const speech = screen.getByRole("switch", { name: "Silenciar Jarvito" });
    expect(sound).not.toBeChecked(); expect(speech).not.toBeChecked();
    await user.click(speech);
    await waitFor(() => expect(speech).toBeChecked());
    expect(sound).not.toBeChecked();
    expect(call).toHaveBeenCalledWith("set_companion_speech", { enabled: false });
    expect(call).not.toHaveBeenCalledWith("set_companion_sound", { enabled: false });
    await user.click(sound);
    await waitFor(() => expect(sound).toBeChecked());
    expect(speech).toBeChecked();
  });

  it("restores voice volume and persists keyboard adjustment independently of both mute switches", async () => {
    measureSliders();
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => command === "get_companion_speech_volume" ? 0.42
      : command === "set_companion_speech_volume" ? Math.fround((args as { volume: number }).volume)
      : command === "set_companion_speech" ? (args as { enabled: boolean }).enabled : original?.(command, args));
    const user = await expanded();
    await user.click(screen.getByRole("button", { name: "Sons e fala do Jarvito" }));
    const slider = await screen.findByRole("slider", { name: "Volume da voz" });
    expect(slider).toHaveAttribute("aria-valuenow", "42");
    expect(slider).toHaveAttribute("min", "0"); expect(slider).toHaveAttribute("max", "100");
    expect(screen.getByText("42%")).toHaveClass("font-mono");
    expect(slider.closest("[data-slot=slider]")).toHaveClass("cursor-pointer");
    slider.focus();
    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_speech_volume", { volume: 0.43 }));
    expect(slider).toHaveAttribute("aria-valuenow", "43");
    expect(screen.getByText("43%")).toBeVisible();
    const sound = screen.getByRole("switch", { name: "Silenciar sons" });
    const speech = screen.getByRole("switch", { name: "Silenciar Jarvito" });
    expect(sound).not.toBeChecked(); expect(speech).not.toBeChecked();
    expect(call).not.toHaveBeenCalledWith("set_companion_sound", expect.anything());
    expect(call).not.toHaveBeenCalledWith("set_companion_speech", expect.anything());
    await user.click(speech);
    await waitFor(() => expect(speech).toBeChecked());
    expect(slider).toHaveAttribute("aria-valuenow", "43");
    expect(slider).not.toBeDisabled();
    expect(sound).not.toBeChecked();
  });

  it("restores the confirmed volume and shows feedback if saving fails", async () => {
    measureSliders();
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "get_companion_speech_volume") return 0.4;
      if (command === "set_companion_speech_volume") throw new Error("Disk full");
      return original?.(command, args);
    });
    const user = await expanded();
    await user.click(screen.getByRole("button", { name: "Sons e fala do Jarvito" }));
    const slider = await screen.findByRole("slider", { name: "Volume da voz" });
    slider.focus(); await user.keyboard("{ArrowLeft}");
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_speech_volume", { volume: 0.39 }));
    await waitFor(() => expect(slider).toHaveAttribute("aria-valuenow", "40"));
    expect(slider).not.toBeDisabled();
    expect(screen.getByText("40%")).toBeVisible();
    expect(screen.getByRole("alert", { name: "" })).toHaveTextContent("Não foi possível salvar o volume da voz.");
    expect(screen.getByRole("switch", { name: "Silenciar Jarvito" })).not.toBeChecked();
  });

  it("reserves native window space for the sound popover and restores the compact activity height afterward", async () => {
    measureSliders();
    const user = await expanded();
    const trigger = screen.getByRole("button", { name: "Sons e fala do Jarvito" });
    await user.click(trigger);
    await screen.findByRole("slider", { name: "Volume da voz" });
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 256 }));
    call.mockClear();
    await user.click(trigger);
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 160 }));
  });

  it("reacts to pet taps without resizing, dragging or toggling the expanded island", async () => {
    await expanded();
    vi.useFakeTimers(); call.mockClear();
    const pet = screen.getByRole("button", { name: "Interagir com Jarvito" });
    fireEvent.pointerDown(pet, { button: 0, screenX: 100 });
    fireEvent.pointerMove(pet, { buttons: 1, screenX: 140 });
    fireEvent.pointerUp(pet);
    fireEvent.click(pet);
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-gesture", "poke");
    fireEvent.click(pet); fireEvent.click(pet);
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-gesture", "dizzy");
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    fireEvent.click(pet); fireEvent.click(pet);
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-gesture", "dizzy");
    await act(async () => { await vi.advanceTimersByTimeAsync(999); });
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-gesture", "dizzy");
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(pet.querySelector("svg[data-state]")).toHaveAttribute("data-gesture", "none");
    expect(screen.getByRole("region", { name: "Assistente Jarvis" })).toBeVisible();
    expect(call.mock.calls.some(([command]) => /set_companion_expanded|companion_start_drag|companion_move_horizontal/.test(command))).toBe(false);
  });

  it("keeps the larger native canvas until the detail pane has folded back into the dashboard", async () => {
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Limites" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 400 }));
    vi.useFakeTimers(); call.mockClear();
    fireEvent.click(screen.getByRole("tab", { name: "Atividade" }));
    expect(screen.getByRole("heading", { name: base.title })).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 160 });
    await act(async () => { await vi.advanceTimersByTimeAsync(360); });
    expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 160 });
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
    expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).querySelector("svg[data-state=waiting]")).toBeInTheDocument();
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
    snapshot.items = Array.from({ length: 31 }, (_, index) => ({ ...base, conversationId: `archive-${index}`, status: "completed", acknowledged: true, activeSince: null, updatedAt: index, title: `Conversa anterior ${index}` }));
    snapshot.items.push({ ...base, conversationId: "current", title: "Implementação atual" });
    await expanded();
    expect(screen.getByRole("heading", { name: "Implementação atual" })).toBeVisible();
    expect(screen.queryByRole("combobox", { name: "Conversa ou agente" })).not.toBeInTheDocument();
    expect(screen.queryByText("current/root")).not.toBeInTheDocument();
    expect(screen.queryByText("Conversa anterior 30")).not.toBeInTheDocument();
  });

  it("prioritizes new work over old failures and keeps the most recent terminal result", async () => {
    snapshot.items = [{ ...base, status: "failed", activeSince: null, updatedAt: 100, title: "Falha antiga" }, { ...base, conversationId: "new-conversation", updatedAt: 1_000_000, title: "Trabalho novo" }];
    await expanded();
    expect(screen.getByText("Trabalho novo")).toBeVisible();
    expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).querySelector("svg[data-state=running]")).toBeInTheDocument();
    snapshot.items[1] = { ...snapshot.items[1], status: "completed", activeSince: null, updatedAt: 1_010_000 };
    act(() => events.get("companion:changed")?.({}));
    await waitFor(() => expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).querySelector("svg[data-state=completed]")).toBeInTheDocument());
    expect(await screen.findByRole("heading", { name: "Trabalho novo" })).toBeVisible();
  });

  it("opens an active lateral entry without dismissing a pending terminal notification", async () => {
    const done = { ...base, status: "completed" as const, activeSince: null, attentionId: "previous/completed", updatedAt: 100, title: "Resultado anterior" };
    const running = { ...base, conversationId: "active-chat", title: "Implementação em andamento" };
    snapshot.items = [done, running];
    const user = await expanded();
    expect(await screen.findByRole("heading", { name: done.title })).toBeVisible();
    const activities = screen.getByLabelText("Outras atividades");
    const action = within(activities).getByRole("button", { name: `Ver atividade: ${running.title}` });
    expect(action).toHaveTextContent("Ver atividade");
    await user.click(action);
    await waitFor(() => expect(call).toHaveBeenCalledWith("companion_open_conversation", { conversationId: running.conversationId }));
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
    expect(snapshot.items.find(item => item.attentionId === done.attentionId)?.acknowledged).toBe(false);
    expect(screen.getByRole("heading", { name: done.title })).toBeVisible();
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
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
    await user.click(await screen.findByRole("button", { name: "Ver atividade" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: base.conversationId, agentId: null, attentionId: "first/completed", revision: 1 }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).querySelector("svg[data-state=failed]")).toBeInTheDocument());
    await waitFor(() => expect(screen.queryByRole("heading", { name: base.title })).not.toBeInTheDocument());
    expect(screen.getByRole("heading", { name: "Outro relatório" })).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("ack_companion_item", expect.objectContaining({ attentionId: "other/failed", revision: 1 }));
    await user.click(await screen.findByRole("button", { name: "Ver atividade" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: "other-chat", agentId: null, attentionId: "other/failed", revision: 1 }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).querySelector("svg[data-state=idle]")).toBeInTheDocument());
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Outro relatório" })).not.toBeInTheDocument());
    expect(screen.getByRole("heading", { name: "Tudo tranquilo por aqui" })).toBeVisible();
  });

  it("communicates the actual response in a global chat instead of announcing the pet's name", async () => {
    snapshot.items = [{ ...base, title: "Jarvito", projectName: "Chat geral", status: "completed", activeSince: null, result: "O próximo feriado será em 12 de outubro." }];
    render(<Companion />);
    const speech = await screen.findByRole("status", { name: "Aviso do Jarvito" });
    expect(speech).toHaveTextContent("Sua resposta está pronta.");
    await waitFor(() => expect(speech).toHaveTextContent("O próximo feriado será em 12 de outubro."));
    expect(speech).not.toHaveTextContent("Concluí: Jarvito");
  });

  it.each(["speech", "home"])("OK in the %s dismisses only its result without opening a conversation", async surface => {
    const first = { ...base, status: "completed" as const, activeSince: null, attentionId: "first/completed", updatedAt: 200 };
    const second = { ...first, conversationId: "other-chat", title: "Outro relatório", attentionId: "other/completed", updatedAt: 100 };
    snapshot.items = [first, second];
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "ack_companion_item") {
        const attentionId = (args as { attentionId: string }).attentionId;
        snapshot = { ...snapshot, items: snapshot.items.map(item => item.attentionId === attentionId ? { ...item, acknowledged: true } : item) };
        return snapshot;
      }
      return original?.(command, args);
    });
    const user = userEvent.setup(); render(<Companion />);
    let expected = second;
    if (surface === "home") {
      await user.click(await screen.findByRole("button", { name: /Abrir assistente Jarvis/ }));
      expected = first;
    } else await screen.findByRole("status", { name: "Aviso do Jarvito" });
    await user.click(await screen.findByRole("button", { name: "OK" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: expected.conversationId, agentId: null, attentionId: expected.attentionId, revision: 1 }));
    expect(call.mock.calls.some(([command]) => command === "companion_open_conversation" || command === "get_companion_chat")).toBe(false);
    expect(snapshot.items.filter(item => !item.acknowledged)).toHaveLength(1);
    if (surface === "home") await waitFor(() => expect(screen.queryByRole("heading", { name: base.title })).not.toBeInTheDocument());
    else expect(screen.getByRole("status", { name: "Aviso do Jarvito" })).toHaveTextContent(base.title);
  });

  it("opens a general result inside the island and only acknowledges after its transcript loads", async () => {
    snapshot.items = [{ ...base, global: true, conversationId: "global-chat", title: "Jarvito", projectName: "Chat geral", status: "completed", activeSince: null }];
    const chat = {
      conversationId: "global-chat", projectId: null, projectName: null, global: true,
      chat: { conversationId: "global-chat", revision: 1, activeTurnId: null, pendingApproval: null, turns: [{
        id: "general-turn", createdAt: 1, durationMs: 10, user: "Qual o próximo feriado?", status: "completed", error: null,
        options: { account: "codex", model: "gpt-6", reasoning: null, mode: "build", approvalMode: "yolo" },
        steps: [{ durationMs: 10, text: "O próximo feriado será em 12 de outubro.", summary: "", tools: [], usage: null }],
      }] },
    };
    let resolveChat: ((value: unknown) => void) | undefined;
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "get_companion_chat") return new Promise(resolve => { resolveChat = resolve; });
      if (command === "get_companion_conversations" || command === "get_companion_models") return [];
      if (command === "ack_companion_item") {
        snapshot = { ...snapshot, items: snapshot.items.map(item => ({ ...item, acknowledged: true })) };
        return snapshot;
      }
      return original?.(command, args);
    });
    const user = userEvent.setup(); render(<Companion />);
    await user.click(await screen.findByRole("button", { name: /Abrir assistente Jarvis/ }));
    await user.click(await screen.findByRole("button", { name: "Ver atividade" }));
    await waitFor(() => expect(resolveChat).toBeDefined());
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
    const pending = resolveChat;
    call.mockImplementation(async (command, args) => {
      if (command === "get_companion_chat") return chat;
      if (command === "get_companion_conversations" || command === "get_companion_models") return [];
      if (command === "ack_companion_item") {
        snapshot = { ...snapshot, items: snapshot.items.map(item => ({ ...item, acknowledged: true })) };
        return snapshot;
      }
      return original?.(command, args);
    });
    await act(async () => pending?.(chat));
    expect(await screen.findByText("O próximo feriado será em 12 de outubro.")).toBeVisible();
    expect(screen.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
    expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: "global-chat", agentId: null, attentionId: base.attentionId, revision: 1 });
    expect(call.mock.calls.some(([command]) => command === "companion_open_conversation")).toBe(false);
    await user.click(screen.getByRole("tab", { name: "Atividade" }));
    expect(screen.getByRole("heading", { name: "Tudo tranquilo por aqui" })).toBeVisible();
  });

  it("keeps an unseen result in the inbox when navigation fails", async () => {
    snapshot.items = [{ ...base, status: "failed", activeSince: null }];
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => command === "companion_open_conversation" ? Promise.reject("Não foi possível acessar a conversa.") : original?.(command, args));
    const user = userEvent.setup(); render(<Companion />);
    await user.click(await screen.findByRole("button", { name: /Abrir assistente Jarvis/ }));
    await user.click(await screen.findByRole("button", { name: "Ver atividade" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível acessar a conversa.");
    expect(screen.getByRole("heading", { name: base.title })).toBeVisible();
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
  });

  it.each(["invalid", "stale"])("preserves the inbox and permits retry after an %s acknowledgement", async kind => {
    snapshot.items = [{ ...base, status: "completed", activeSince: null }];
    let acknowledgements = 0;
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "ack_companion_item") {
        if (++acknowledgements === 1) {
          if (kind === "invalid") return true;
          snapshot = { ...snapshot, items: snapshot.items.map(item => ({ ...item, revision: 2, title: "Resultado atualizado" })) };
          return snapshot;
        }
        snapshot = { ...snapshot, items: snapshot.items.map(item => ({ ...item, acknowledged: true })) };
        return snapshot;
      }
      return original?.(command, args);
    });
    const user = userEvent.setup(); render(<Companion />);
    await user.click(await screen.findByRole("button", { name: /Abrir assistente Jarvis/ }));
    await user.click(await screen.findByRole("button", { name: "Ver atividade" }));
    const title = kind === "invalid" ? base.title : "Resultado atualizado";
    expect(await screen.findByRole("heading", { name: title })).toBeVisible();
    if (kind === "invalid") expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível confirmar que a atividade foi vista.");
    await user.click(await screen.findByRole("button", { name: "Ver atividade" }));
    await waitFor(() => expect(acknowledgements).toBe(2));
    expect(call.mock.calls.filter(([command]) => command === "ack_companion_item")).toEqual([
      ["ack_companion_item", { conversationId: base.conversationId, agentId: null, attentionId: base.attentionId, revision: 1 }],
      ["ack_companion_item", { conversationId: base.conversationId, agentId: null, attentionId: base.attentionId, revision: kind === "invalid" ? 1 : 2 }],
    ]);
    expect(await screen.findByRole("heading", { name: "Tudo tranquilo por aqui" })).toBeVisible();
  });

  it.each([true, false])("only announces a general chat response when it was not viewed in the island: viewed=%s", async viewed => {
    snapshot.items = [{ ...base, conversationId: "global-chat", title: "Jarvito", projectName: "Chat geral" }];
    const chat = {
      conversationId: "global-chat", projectId: null, projectName: null, global: true, proposal: null,
      chat: { conversationId: "global-chat", revision: 1, turns: [], activeTurnId: "turn-1" as string | null, pendingApproval: null },
    };
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "get_companion_chat") return chat;
      if (command === "get_companion_conversations" || command === "get_companion_models") return [];
      if (command === "ack_companion_item") {
        snapshot.items = snapshot.items.map(item => ({ ...item, acknowledged: true }));
        return snapshot;
      }
      return original?.(command, args);
    });
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Chat" }));
    await screen.findByRole("textbox", { name: "Mensagem para Jarvito" });
    if (!viewed) {
      fireEvent.keyDown(screen.getByRole("region", { name: "Assistente Jarvis" }), { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("region", { name: "Assistente Jarvis" })).not.toBeInTheDocument());
    }
    snapshot.items = [{ ...snapshot.items[0], status: "completed", activeSince: null, attentionId: "general/completed", revision: 2, result: "A resposta da sua pergunta." }];
    chat.chat.revision++; chat.chat.activeTurnId = null;
    act(() => {
      events.get("companion:changed")?.({});
      events.get("companion:chat_changed")?.({ conversationId: "global-chat" });
    });
    if (viewed) {
      await waitFor(() => expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: "global-chat", agentId: null, attentionId: "general/completed", revision: 2 }));
      await waitFor(() => expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).querySelector("svg[data-state=idle]")).toBeInTheDocument());
      fireEvent.keyDown(screen.getByRole("region", { name: "Assistente Jarvis" }), { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("region", { name: "Assistente Jarvis" })).not.toBeInTheDocument());
      expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
    } else {
      expect(await screen.findByRole("status", { name: "Aviso do Jarvito" })).toHaveTextContent("A resposta da sua pergunta.");
      expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
    }
  });

  it("does not mark a new response as viewed until the corresponding transcript is displayed", async () => {
    snapshot.items = [{ ...base, conversationId: "global-chat", title: "Jarvito", projectName: "Chat geral" }];
    const makeChat = (revision: number, text: string) => ({
      conversationId: "global-chat", projectId: null, projectName: null, global: true,
      chat: { conversationId: "global-chat", revision, activeTurnId: null, pendingApproval: null, turns: [{
        id: `turn-${revision}`, createdAt: 1, durationMs: 10, user: "Minha pergunta", status: "completed", error: null,
        options: { account: "codex", model: "gpt-6", reasoning: null, mode: "build", approvalMode: "yolo" },
        steps: [{ durationMs: 10, text, summary: "", tools: [], usage: null }],
      }] },
    });
    let chat = makeChat(1, "Resposta anterior");
    let completeRefresh: ((value: unknown) => void) | undefined;
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "get_companion_chat") return chat;
      if (command === "get_companion_conversations" || command === "get_companion_models") return [];
      return original?.(command, args);
    });
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Chat" }));
    await screen.findByText("Resposta anterior");
    const loaded = call.getMockImplementation();
    call.mockImplementation(async (command, args) => command === "get_companion_chat"
      ? new Promise(resolve => { completeRefresh = resolve; }) : loaded?.(command, args));
    snapshot.items = [{ ...snapshot.items[0], status: "completed", revision: 2, activeSince: null, attentionId: "new-response/completed" }];
    act(() => {
      events.get("companion:changed")?.({});
      events.get("companion:chat_changed")?.({ conversationId: "global-chat" });
    });
    await waitFor(() => expect(completeRefresh).toBeDefined());
    await waitFor(() => expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).querySelector("svg[data-state=completed]")).toBeInTheDocument());
    expect(screen.getByText("Resposta anterior")).toBeVisible();
    expect(call.mock.calls.some(([command]) => command === "ack_companion_item")).toBe(false);
    chat = makeChat(2, "Nova resposta pronta");
    await act(async () => completeRefresh?.(chat));
    expect(await screen.findByText("Nova resposta pronta")).toBeVisible();
    await waitFor(() => expect(call).toHaveBeenCalledWith("ack_companion_item", { conversationId: "global-chat", agentId: null, attentionId: "new-response/completed", revision: 2 }));
  });

  it.each(["waiting", "completed", "failed"] as const)("preserves a %s notice while dragging the black header horizontally", async status => {
    snapshot.items = [{ ...base, status, activeSince: null, pendingQuestion: status === "waiting" ? { turnId: "turn-q", toolId: "ask-q", questions: [{ id: "scope", question: "Qual tela revisar?", options: [] }] } : null }];
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      if (command === "companion_finish_drag") {
        events.get("companion:drag-end")?.(null);
        return true;
      }
      return original?.(command, args);
    });
    render(<Companion />);
    await screen.findByRole("status", { name: "Aviso do Jarvito" });
    const header = screen.getByRole("status", { name: "Aviso do Jarvito" }).parentElement?.querySelector(".companion-header");
    if (!header) throw new Error("Missing black header");
    fireEvent.pointerDown(header, { button: 0, screenX: 100 });
    fireEvent.pointerMove(header, { screenX: 140, buttons: 1 });
    await waitFor(() => expect(call).toHaveBeenCalledWith("companion_start_drag"));
    expect(screen.getByRole("status", { name: "Aviso do Jarvito" })).toBeVisible();
    const bubbleRequests = call.mock.calls.filter(([command]) => command === "set_companion_bubble").length;
    fireEvent.pointerUp(header);
    expect(call.mock.calls.filter(([command]) => command === "set_companion_bubble")).toHaveLength(bubbleRequests);
    await waitFor(() => expect(call).toHaveBeenCalledWith("companion_finish_drag"));
    await screen.findByRole("status", { name: "Aviso do Jarvito" });
    expect(call.mock.calls.filter(([command]) => command === "set_companion_bubble")).toHaveLength(bubbleRequests);
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
    act(() => events.get("companion:geometry")?.(geometryFor(false)));
    await act(async () => resolveBubble?.({ ...geometryFor(false, 160, true), robotSide: "right", robotVertical: "bottom" }));
    const main = screen.getByRole("button", { name: /Abrir assistente Jarvis/ }).closest("main");
    expect(main).toHaveAttribute("data-robot-side", "left");
    expect(main).toHaveAttribute("data-robot-vertical", "top");
    expect(screen.queryByRole("status", { name: "Aviso do Jarvito" })).not.toBeInTheDocument();
  });

  it("animates outside-click collapse before shrinking the native island", async () => {
    await expanded();
    vi.useFakeTimers(); call.mockClear();
    act(() => events.get("companion:collapse-request")?.(null));
    const main = screen.getByRole("button", { name: "Interagir com Jarvito" }).closest("main");
    expect(main).toHaveAttribute("data-closing", "true");
    expect(screen.getByRole("region", { name: "Assistente Jarvis" })).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("set_companion_expanded", { expanded: false });
    await act(async () => { await vi.advanceTimersByTimeAsync(360); });
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
    const geometry = geometryFor(true);
    act(() => {
      events.get("companion:geometry")?.(geometry);
      events.get("companion:collapse-request")?.(null);
    });
    await act(async () => finishOpen?.(geometry));
    expect(screen.getByRole("button", { name: "Interagir com Jarvito" }).closest("main")).toHaveAttribute("data-closing", "true");
    await act(async () => { await vi.advanceTimersByTimeAsync(360); });
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
    expect(await screen.findByText("72%")).toBeVisible();
    expect(screen.getByRole("progressbar", { name: "Codex 5h restante" })).toHaveAttribute("aria-valuenow", "72");
    const count = () => call.mock.calls.filter(([command]) => command === "get_companion_usage").length;
    expect(count()).toBe(1);
    act(() => events.get("companion:usage")?.({}));
    await waitFor(() => expect(count()).toBe(2));
    expect(call.mock.calls.filter(([command]) => command === "get_companion_usage").slice(-1)[0]).toEqual(["get_companion_usage", undefined]);
    await user.click(screen.getByRole("button", { name: "Atualizar limites" }));
    await waitFor(() => expect(count()).toBe(3));
    expect(call.mock.calls.filter(([command]) => command === "get_companion_usage").slice(-1)[0]).toEqual(["get_companion_usage", { refresh: true }]);
    const panel = screen.getByRole("tabpanel", { name: "Limites" });
    expect(within(panel).getByText("pessoal")).toBeVisible();
  });

  it("identifies providers with icons while preserving shortened and custom aliases", async () => {
    const providers = [
      { alias: "openai-codex-pessoal", providerKind: "openai-codex", label: "pessoal", name: "OpenAI Codex" },
      { alias: "antigravity-pessoal", providerKind: "antigravity", label: "pessoal", name: "Antigravity" },
      { alias: "opencode-go-pessoal", providerKind: "opencode-go", label: "pessoal", name: "OpenCode Go" },
      { alias: "Claude Code", providerKind: "claude-code", label: "Claude Code", name: "Claude Code" },
      { alias: "conta do trabalho", providerKind: "antigravity", label: "conta do trabalho", name: "Antigravity" },
    ];
    accounts = providers.map(provider => ({ ...account, alias: provider.alias, providerKind: provider.providerKind }));
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Limites" }));
    for (const provider of providers) {
      const section = await screen.findByRole("region", { name: `Limites de ${provider.alias}` });
      expect(within(section).getByRole("img", { name: provider.name })).toBeVisible();
      expect(within(section).getByText(provider.label)).toBeVisible();
    }
  });

  it("keeps both periods, resets and quota pace readable for multiple providers", async () => {
    accounts = [
      { ...account, windows: [
        { ...account.windows[0], resetsAt: Date.now() + 9_000_000 },
        { ...account.windows[0], id: "weekly", label: "7d", durationSeconds: 604_800, remainingPercent: 76, resetsAt: Date.now() + 6.5 * 86_400_000 },
      ] },
      { ...account, alias: "Claude Code", providerKind: "claude-code", windows: [
        { ...account.windows[0], group: "Claude", remainingPercent: 7, resetsAt: Date.now() + 9_000_000 },
        { ...account.windows[0], id: "weekly", group: "Claude", label: "7d", durationSeconds: 604_800, remainingPercent: 83, resetsAt: Date.now() + 3.5 * 86_400_000 },
      ] },
    ];
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Limites" }));
    for (const provider of accounts) {
      const section = await screen.findByRole("region", { name: `Limites de ${provider.alias}` });
      expect(within(section).getAllByRole("progressbar")).toHaveLength(2);
      expect(within(section).getByText("5h")).toBeVisible();
      expect(within(section).getByText("7d")).toBeVisible();
      expect(within(section).getAllByText(/Renova em/)).toHaveLength(2);
      expect(within(section).getByLabelText(/Atualizado às/)).toBeVisible();
    }
    const claude = screen.getByRole("region", { name: "Limites de Claude Code" });
    expect(within(claude).getByText("43% em déficit")).toBeVisible();
    expect(within(claude).getByText("33% em reserva")).toBeVisible();
    expect(within(claude).getByRole("progressbar", { name: "Claude 5h restante" })).toHaveAttribute("aria-valuenow", "7");
    expect(within(claude).getByRole("img", { name: "Claude Code" })).toBeVisible();
  });

  it.each([
    { remaining: 78, days: 3.5, expected: "28% em reserva" },
    { remaining: 76, days: 6.5, expected: "17% em déficit" },
    { remaining: 50, days: 3.5, expected: "No ritmo da janela" },
    { remaining: 78, days: 3.5, error: "offline", expected: null },
    { remaining: 78, days: 3.5, ageMs: 300_001, expected: null },
    { remaining: 78, days: 3.5, durationSeconds: null, expected: null },
  ])("shows the same quota pace as the status bar for current windows: $expected", async ({ remaining, days, expected, error = null, ageMs = 0, durationSeconds = 604_800 }) => {
    accounts = [{ ...account, error, fetchedAt: Date.now() - ageMs, windows: [{ ...account.windows[0], remainingPercent: remaining, durationSeconds, resetsAt: Date.now() + days * 86_400_000 }] }];
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Limites" }));
    await screen.findByText(`${remaining}%`);
    const panel = screen.getByRole("tabpanel", { name: "Limites" });
    if (expected) {
      expect(within(panel).getByText(expected)).toBeVisible();
      expect(within(panel).getByRole("img", { name: `Restante esperado: ${Math.round(days / 7 * 100)}%` })).toBeVisible();
    } else {
      expect(within(panel).queryByText(/% em (reserva|déficit)|No ritmo da janela/)).not.toBeInTheDocument();
      expect(within(panel).queryByRole("img", { name: /Restante esperado/ })).not.toBeInTheDocument();
    }
  });

  it("preserves cached limits but hides pace estimates after a failed refresh", async () => {
    const user = await expanded();
    await user.click(screen.getByRole("tab", { name: "Limites" }));
    await screen.findByText("72%");
    expect(screen.getByText("71% em reserva")).toBeVisible();
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => command === "get_companion_usage" && (args as { refresh?: boolean } | undefined)?.refresh ? Promise.reject("Sem conexão") : original?.(command, args));
    await user.click(screen.getByRole("button", { name: "Atualizar limites" }));
    await screen.findByText("Sem conexão");
    expect(screen.getByText("72%")).toBeVisible();
    expect(screen.queryByText(/% em (reserva|déficit)/)).not.toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /Restante esperado/ })).not.toBeInTheDocument();
  });

  it("distinguishes dragging from clicking and follows geometry without switching the app", async () => {
    render(<Companion />);
    const compact = await screen.findByRole("button", { name: "Abrir ilha do Jarvito" });
    fireEvent.pointerDown(compact, { button: 0, screenX: 100 });
    fireEvent.pointerMove(compact, { screenX: 140, buttons: 1 });
    fireEvent.pointerUp(compact, { screenX: 140 });
    fireEvent.click(compact, { detail: 1 });
    await waitFor(() => expect(call).toHaveBeenCalledWith("companion_start_drag"));
    await waitFor(() => expect(call).toHaveBeenCalledWith("companion_move_horizontal"));
    expect(call).not.toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 160 });
    act(() => events.get("companion:geometry")?.({ expanded: true, bubble: false, robotSide: "left", robotVertical: "top", width: 420, height: 520 }));
    const panel = await screen.findByRole("region", { name: "Assistente Jarvis" });
    const movedPet = screen.getByRole("button", { name: "Interagir com Jarvito" });
    expect(movedPet.closest(".companion-island")).toContainElement(panel);
    expect(screen.getByRole("tab", { name: "Chat" })).toBeVisible();
    expect(call).not.toHaveBeenCalledWith("companion_open_conversation", expect.anything());
    fireEvent.keyDown(panel, { key: "Escape" });
    await waitFor(() => expect(screen.getByRole("button", { name: /Abrir assistente Jarvis/ })).toHaveAttribute("aria-expanded", "false"));
  });

  it("finishes horizontal dragging only after the native start and move have completed", async () => {
    let startDrag: ((value: unknown) => void) | undefined;
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => command === "companion_start_drag" ? new Promise(resolve => { startDrag = resolve; }) : original?.(command, args));
    render(<Companion />);
    const compact = await screen.findByRole("button", { name: "Abrir ilha do Jarvito" });
    fireEvent.pointerDown(compact, { button: 0, screenX: 100 });
    fireEvent.pointerMove(compact, { screenX: 140, buttons: 1 });
    fireEvent.pointerUp(compact, { screenX: 140 });
    await waitFor(() => expect(startDrag).toBeDefined());
    expect(call.mock.calls.some(([command]) => command === "companion_finish_drag" || command === "companion_move_horizontal")).toBe(false);
    await act(async () => startDrag?.(true));
    await waitFor(() => expect(call).toHaveBeenCalledWith("companion_finish_drag"));
    expect(call.mock.calls.filter(([command]) => /^companion_(start_drag|move_horizontal|finish_drag)$/.test(command)).map(([command]) => command)).toEqual(["companion_start_drag", "companion_move_horizontal", "companion_finish_drag"]);
  });

  it("reserves the Mac camera header and keeps the island fixed when its header or pet is dragged", async () => {
    const original = call.getMockImplementation();
    call.mockImplementation(async (command, args) => {
      const result = await original?.(command, args);
      if (command !== "set_companion_expanded" && command !== "set_companion_bubble") return result;
      if (!result || typeof result !== "object") throw new Error("Missing geometry");
      return { ...result, compactWidth: 314, compactHeight: 38, notchWidth: 210, notchHeight: 38, headerHeight: 38, dragAxis: "none" };
    });
    const user = userEvent.setup(); render(<Companion />);
    await user.click(await screen.findByRole("button", { name: "Abrir ilha do Jarvito" }));
    await screen.findByRole("region", { name: "Assistente Jarvis" });
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 198 }));
    const pet = screen.getByRole("button", { name: "Interagir com Jarvito" });
    const main = pet.closest("main");
    expect(main?.style.getPropertyValue("--companion-notch-inset")).toBe("38px");
    const header = main?.querySelector(".companion-header");
    if (!header) throw new Error("Missing header");
    for (const target of [pet, header]) {
      fireEvent.pointerDown(target, { button: 0, screenX: 100 });
      fireEvent.pointerMove(target, { screenX: 140, buttons: 1 });
      fireEvent.pointerUp(target);
    }
    expect(call.mock.calls.some(([command]) => /^companion_(start_drag|move_horizontal|finish_drag)$/.test(command))).toBe(false);
    await user.click(screen.getByRole("tab", { name: "Chat" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("set_companion_expanded", { expanded: true, height: 438 }));
  });

  it("keeps controls usable when native placement provides a smaller panel", async () => {
    await expanded();
    act(() => events.get("companion:geometry")?.({ expanded: true, bubble: false, robotSide: "left", robotVertical: "bottom", width: 300, height: 320 }));
    expect(screen.getByRole("heading", { name: base.title })).toBeVisible();
    expect(screen.getByRole("button", { name: "Abrir conversa no Jarvis" })).toBeEnabled();
    fireEvent.keyDown(screen.getByRole("region", { name: "Assistente Jarvis" }), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("region", { name: "Assistente Jarvis" })).not.toBeInTheDocument());
  });

  it("keeps the robot inside the island and follows the pointer without requesting focus", async () => {
    await expanded();
    const pet = screen.getByRole("button", { name: "Interagir com Jarvito" });
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
