import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { idleVoice, voiceSession, voiceSettings, voiceTarget } from "@/test/voice-fixtures";
import type { VoiceSession } from "@/core/voice";
import { VoiceControls, VoiceSessionPanel } from "./VoiceControls";

const mock = vi.hoisted(() => ({ start: vi.fn(), control: vi.fn(), save: vi.fn(), refresh: vi.fn(), acceptsDictation: vi.fn(), session: null as VoiceSession | null, active: false, settings: null as ReturnType<typeof voiceSettings> | null, events: new Map<string, (value: unknown) => void>() }));
vi.mock("@/hooks/use-voice", () => ({ acceptsDictationTranscript: mock.acceptsDictation, useVoice: () => ({ ...mock, loading: false, error: null }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name: string, callback: (event: { payload: unknown }) => void) => { mock.events.set(name, payload => callback({ payload })); return () => mock.events.delete(name); }) }));
const transcript = (text: string, sequence = 1, target = voiceTarget) => ({ sessionId: "voice-1", target, mode: "dictation", sequence, text });
const emit = async (value: unknown) => { await act(async () => { mock.events.get("voice:transcript")?.(value); }); };

describe("dictation controls", () => {
  beforeEach(() => { mock.session = { ...idleVoice }; mock.active = false; mock.settings = voiceSettings(); mock.events.clear(); mock.start.mockReset().mockResolvedValue(voiceSession()); mock.control.mockReset().mockResolvedValue(undefined); mock.acceptsDictation.mockReset().mockReturnValue(true); });
  afterEach(() => vi.restoreAllMocks());
  it("starts dictation explicitly and exposes no call controls", async () => {
    render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    expect(mock.start).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: /Ligar|Encerrar ligação/ })).not.toBeInTheDocument();
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Ditar mensagem" })); });
    expect(mock.start).toHaveBeenCalledExactlyOnceWith(voiceTarget, "dictation");
  });
  it("appends dictation and rejects duplicated, foreign, removed and cancelled transcripts", async () => {
    mock.session = voiceSession(); mock.active = true;
    const onDictation = vi.fn();
    render(<VoiceControls target={voiceTarget} onDictation={onDictation} />);
    await emit(transcript("Meu texto")); await emit(transcript("Duplicado"));
    await emit(transcript("Outro chat", 2, "chat:other"));
    await emit({ ...transcript("Chamada antiga", 2), mode: "call" });
    await emit({ ...transcript("Outra sessão", 2), sessionId: "foreign" });
    expect(onDictation).toHaveBeenCalledExactlyOnceWith("Meu texto");
    mock.acceptsDictation.mockReturnValue(false);
    await emit(transcript("Fala anterior entregue após o envio", 3));
    expect(onDictation).toHaveBeenCalledTimes(1);
  });
  it("uses the microphone-off control and allows finishing while the chat is busy", async () => {
    mock.session = voiceSession(); mock.active = true;
    render(<VoiceControls disabled target={voiceTarget} onDictation={vi.fn()} />);
    const finish = screen.getByRole("button", { name: "Concluir ditado" });
    expect(finish).toBeEnabled();
    expect(finish.querySelector(".lucide-mic-off")).not.toBeNull();
    expect(finish.querySelector(".lucide-square")).toBeNull();
    await act(async () => { fireEvent.click(finish); });
    expect(mock.control).toHaveBeenCalledWith("voice-1", "finish");
  });
  it("cancels preparation and provides keyboard control for only its own dictation", async () => {
    const view = render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    await act(async () => { fireEvent.keyDown(document, { code: "Space", ctrlKey: true, shiftKey: true }); });
    expect(mock.start).toHaveBeenCalledWith(voiceTarget, "dictation");
    mock.session = voiceSession({ phase: "preparing" }); mock.active = true;
    view.rerender(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Concluir ditado" })); });
    expect(mock.control).toHaveBeenCalledWith("voice-1", "end");
    mock.control.mockClear();
    await act(async () => { fireEvent.keyDown(document, { key: "Escape" }); });
    expect(mock.control).toHaveBeenCalledWith("voice-1", "end");
    mock.session = voiceSession({ target: "chat:other" });
    view.rerender(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    mock.control.mockClear();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(mock.control).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Ditar mensagem" })).toBeDisabled();
    fireEvent.keyDown(document, { code: "Space", metaKey: true, shiftKey: true });
    expect(mock.start).toHaveBeenCalledTimes(1);
  });
  it("opens setup when the local model is absent without starting capture", () => {
    mock.settings = voiceSettings({ models: [] });
    render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Ditar mensagem" }));
    expect(screen.getByRole("dialog")).toBeVisible(); expect(mock.start).not.toHaveBeenCalled();
  });
  it("shows dictation feedback without an avatar or call actions", () => {
    mock.session = voiceSession({ level: 0.6 }); mock.active = true;
    const view = render(<VoiceSessionPanel target={voiceTarget} />);
    expect(screen.getByRole("status")).toHaveTextContent("Estou ouvindo");
    expect(screen.getByText("Uma pausa insere sua fala no campo.")).toBeVisible();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    mock.session = voiceSession({ mode: "announcement" });
    view.rerender(<VoiceSessionPanel target={voiceTarget} />);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
  it("closes owned capture on unmount and leaves a replacement session running", async () => {
    mock.session = voiceSession(); mock.active = true;
    const first = render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    first.unmount();
    expect(mock.control).toHaveBeenCalledWith("voice-1", "end");
    mock.control.mockClear();
    const replacement = render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    mock.session = voiceSession({ id: "replacement" });
    replacement.rerender(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    replacement.unmount();
    expect(mock.control).not.toHaveBeenCalled();
    await waitFor(() => expect(mock.events.has("voice:transcript")).toBe(false));
  });
  it("closes capture started for a previous target and enables dictation in the new composer", async () => {
    let finish: (session: VoiceSession) => void = () => {};
    mock.start.mockImplementationOnce(() => new Promise<VoiceSession>(resolve => { finish = resolve; }));
    const view = render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Ditar mensagem" }));
    view.rerender(<VoiceControls target="chat:new-chat" onDictation={vi.fn()} />);
    await act(async () => { finish(voiceSession({ id: "old-target-capture" })); });
    expect(mock.control).toHaveBeenCalledWith("old-target-capture", "end");
    const start = screen.getByRole("button", { name: "Ditar mensagem" });
    expect(start).toBeEnabled();
    await act(async () => { fireEvent.click(start); });
    expect(mock.start).toHaveBeenLastCalledWith("chat:new-chat", "dictation");
  });
  it("closes capture that finishes starting after its composer unmounts", async () => {
    let finish: (session: VoiceSession) => void = () => {};
    mock.start.mockImplementationOnce(() => new Promise<VoiceSession>(resolve => { finish = resolve; }));
    const view = render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Ditar mensagem" }));
    view.unmount();
    await act(async () => { finish(voiceSession({ id: "late-capture" })); });
    expect(mock.control).toHaveBeenCalledWith("late-capture", "end");
  });
});
