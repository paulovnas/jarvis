import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { companionChatSchema } from "@/core/companion";
import type { VoiceSession } from "@/core/voice";
import { idleVoice, voiceSession, voiceSettings } from "@/test/voice-fixtures";
import { CompanionChatPane } from "./CompanionChatPane";

const voice = vi.hoisted(() => ({ active: false, session: null as VoiceSession | null, control: vi.fn(), start: vi.fn(), stopDictation: vi.fn() }));
vi.mock("@/hooks/use-voice", () => ({ stopDictation: voice.stopDictation, acceptsDictationTranscript: () => true, useVoice: () => ({ ...voice, settings: voiceSettings(), loading: false, error: null }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string) => {
  if (command === "get_companion_chat" || command === "send_companion_message") return companionChatSchema.parse({ conversationId: "general", global: true, projectId: null, projectName: null, proposal: null, chat: { conversationId: "general", revision: 1, turns: [], activeTurnId: null, pendingApproval: null } });
  return [];
}) }));
vi.mock("@/components/companion/Robot", () => ({ Robot: () => <span data-testid="call-avatar" /> }));
vi.mock("@/components/chat/ModelPicker", () => ({ ModelPicker: () => <div aria-label="Modelo do Jarvito">Modelo atual</div> }));

describe("Jarvito call layout", () => {
  beforeEach(() => { voice.active = false; voice.session = { ...idleVoice }; voice.control.mockReset().mockResolvedValue(undefined); voice.stopDictation.mockReset().mockResolvedValue(undefined); vi.mocked(invoke).mockClear(); });
  it("turns dictation off before submitting a manually sent message", async () => {
    render(<CompanionChatPane />);
    const input = await screen.findByRole("textbox", { name: "Mensagem para Jarvito" });
    fireEvent.change(input, { target: { value: "Pode conferir a previsão?" } });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Enviar mensagem" })); });
    expect(voice.stopDictation).toHaveBeenCalledExactlyOnceWith("companion:general");
    const sendIndex = vi.mocked(invoke).mock.calls.findIndex(([command]) => command === "send_companion_message");
    expect(sendIndex).toBeGreaterThanOrEqual(0);
    expect(voice.stopDictation.mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(invoke).mock.invocationCallOrder[sendIndex]);
    expect(input).toHaveValue("");
  });
  it("focuses the island on the call, keeps one control pair and reveals the conversation on demand", async () => {
    const view = render(<CompanionChatPane />);
    await screen.findByRole("textbox", { name: "Mensagem para Jarvito" });
    voice.active = true; voice.session = voiceSession({ target: "companion:general", owner: "companion", transcript: "Olá, estou aqui.", speaker: "jarvis" });
    view.rerender(<CompanionChatPane />);
    expect(screen.getByLabelText("Ligação com Jarvito")).toBeVisible();
    expect(screen.getByText("Jarvito: Olá, estou aqui.")).toBeVisible();
    expect(screen.queryByRole("textbox", { name: "Mensagem para Jarvito" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Modelo do Jarvito")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Limpar conversa" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Encerrar ligação" })).toHaveLength(1);
    expect(screen.getAllByRole("button", { name: "Pausar microfone" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Mostrar conversa" }));
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toBeVisible();
    expect(screen.getByRole("log", { name: "Mensagens do Jarvito" })).toBeVisible();
    expect(screen.getByLabelText("Ligação com Jarvito")).toBeVisible();
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Encerrar ligação" })); });
    await waitFor(() => expect(voice.control).toHaveBeenCalledWith("voice-1", "end"));
    voice.active = false; voice.session = { ...idleVoice };
    view.rerender(<CompanionChatPane />);
    expect(screen.getByRole("textbox", { name: "Mensagem para Jarvito" })).toBeVisible();
    expect(screen.getByLabelText("Modelo do Jarvito")).toBeVisible();
  });
});
