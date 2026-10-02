import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { voiceSettings } from "@/test/voice-fixtures";
import { VoiceSettings } from "./VoiceSettings";
const mock = vi.hoisted(() => ({ settings: null as ReturnType<typeof voiceSettings> | null, error: null as string | null, save: vi.fn(), refresh: vi.fn(), start: vi.fn(), control: vi.fn() }));
vi.mock("@/hooks/use-voice", () => ({ useVoice: () => ({ ...mock, active: false, session: null }) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
describe("local voice setup", () => {
  beforeEach(() => { mock.settings = voiceSettings(); mock.error = null; mock.save.mockReset().mockResolvedValue(undefined); mock.start.mockReset().mockResolvedValue(undefined); mock.refresh.mockReset().mockResolvedValue(undefined); vi.mocked(invoke).mockReset().mockResolvedValue(undefined); });
  afterEach(() => vi.restoreAllMocks());
  it("shows readable device labels and saves explicit activation without opening the mic", async () => {
    mock.settings = voiceSettings(); mock.settings.config.microphone = "mic-1"; mock.settings.config.speaker = "out-1";
    render(<VoiceSettings />);
    expect(screen.getByText("Microfone USB")).toBeVisible(); expect(screen.getByText("Fones de ouvido")).toBeVisible();
    expect(screen.getByRole("group", { name: /Velocidade/ })).toBeVisible();
    expect(screen.getByRole("group", { name: /Pausa para enviar/ })).toBeVisible();
    fireEvent.click(screen.getByRole("switch", { name: "Ativar Jarvis Voice" }));
    await waitFor(() => expect(mock.save).toHaveBeenCalledWith({ ...mock.settings?.config, enabled: false }));
    expect(mock.start).not.toHaveBeenCalled();
  });
  it("downloads only the selected model on demand, preserving failed setup for retry", async () => {
    mock.settings = voiceSettings(); mock.settings.config.model = "tiny";
    vi.mocked(invoke).mockRejectedValue("A conexão caiu. Tente novamente.");
    render(<VoiceSettings />); expect(invoke).not.toHaveBeenCalled();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Baixar modelo" })));
    expect(invoke).toHaveBeenCalledWith("install_voice_model", { model: "tiny" });
    expect(await screen.findByRole("alert")).toHaveTextContent("A conexão caiu");
    expect(screen.getByRole("button", { name: "Baixar modelo" })).toBeEnabled();
  });
  it("shows transfer progress and supports cancellation", async () => {
    mock.settings = voiceSettings({ download: { model: "small", received: 25e6, total: 100e6 } });
    render(<VoiceSettings />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "25");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Cancelar" })));
    expect(invoke).toHaveBeenCalledWith("cancel_voice_download");
  });
  it("keeps dictation available without TTS and explains how to prepare spoken replies", () => {
    mock.settings = voiceSettings({ speechReady: false });
    render(<VoiceSettings />);
    expect(screen.getByRole("button", { name: "Testar voz" })).toBeDisabled();
    expect(screen.getByText(/O ditado funciona/)).toBeVisible();
  });
});
