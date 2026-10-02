import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { idleVoice, voiceSession, voiceSettings, voiceTarget } from "@/test/voice-fixtures";
import type { VoiceSession } from "@/core/voice";
import { VoiceControls, VoiceSessionPanel } from "./VoiceControls";

const mock = vi.hoisted(() => ({ start: vi.fn(), control: vi.fn(), save: vi.fn(), refresh: vi.fn(), session: null as VoiceSession | null, active: false, settings: null as ReturnType<typeof voiceSettings> | null, events: new Map<string, (value: unknown) => void>() }));
vi.mock("@/hooks/use-voice", () => ({ useVoice: () => ({ ...mock, loading: false, error: null }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name: string, callback: (event: { payload: unknown }) => void) => { mock.events.set(name, payload => callback({ payload })); return () => mock.events.delete(name); }) }));
vi.mock("@/components/companion/Robot", () => ({ Robot: () => <span data-testid="voice-robot" /> }));
const transcript = (text: string, mode = "call", sequence = 1, target = voiceTarget) => ({ sessionId: "voice-1", target, mode, sequence, text });
const emit = async (value: unknown) => { await act(async () => { mock.events.get("voice:transcript")?.(value); }); };

describe("voice controls", () => {
  beforeEach(() => { mock.session = { ...idleVoice }; mock.active = false; mock.settings = voiceSettings(); mock.events.clear(); mock.start.mockReset().mockResolvedValue(voiceSession()); mock.control.mockReset().mockResolvedValue(undefined); });
  afterEach(() => vi.restoreAllMocks());
  it("starts explicitly, never reads old replies, and reads a new final response once", async () => {
    const snapshot = { ...emptyChat(), turns: [savedTurn()] };
    const onMessage = vi.fn().mockResolvedValue(true);
    const { rerender } = render(<VoiceControls target={voiceTarget} snapshot={snapshot} onDictation={vi.fn()} onMessage={onMessage} />);
    expect(mock.start).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Ligar para o Jarvis" }));
    await waitFor(() => expect(mock.start).toHaveBeenCalledWith(voiceTarget, "call"));
    mock.session = voiceSession(); mock.active = true;
    rerender(<VoiceControls target={voiceTarget} snapshot={snapshot} onDictation={vi.fn()} onMessage={onMessage} />);
    expect(mock.control).not.toHaveBeenCalled();
    await emit(transcript("Verifique o projeto"));
    expect(onMessage).toHaveBeenCalledWith("Verifique o projeto");
    const next = { ...snapshot, turns: [...snapshot.turns, { ...savedTurn(), id: "earlier-new-turn" }, { ...savedTurn(), id: "new-turn", steps: [{ ...savedTurn().steps[0], text: "Verificação concluída." }] }] };
    rerender(<VoiceControls target={voiceTarget} snapshot={next} onDictation={vi.fn()} onMessage={onMessage} />);
    await waitFor(() => expect(mock.control).toHaveBeenCalledWith("voice-1", "speak", "Verificação concluída."));
    rerender(<VoiceControls target={voiceTarget} snapshot={{ ...next }} onDictation={vi.fn()} onMessage={onMessage} />);
    expect(mock.control.mock.calls.filter(call => call[1] === "speak")).toHaveLength(1);
  });
  it("appends dictation and rejects duplicated or foreign transcripts", async () => {
    mock.session = voiceSession({ mode: "dictation" }); mock.active = true;
    const onDictation = vi.fn(), onMessage = vi.fn();
    render(<VoiceControls target={voiceTarget} onDictation={onDictation} onMessage={onMessage} />);
    await emit(transcript("Meu texto", "dictation")); await emit(transcript("Duplicado", "dictation"));
    await emit(transcript("Outro chat", "dictation", 2, "chat:other"));
    expect(onDictation).toHaveBeenCalledExactlyOnceWith("Meu texto"); expect(onMessage).not.toHaveBeenCalled();
  });
  it("preserves speech if sending fails and resumes listening", async () => {
    mock.session = voiceSession(); mock.active = true;
    const draft = vi.fn();
    render(<VoiceControls target={voiceTarget} onDictation={draft} onMessage={vi.fn().mockResolvedValue(false)} />);
    await emit(transcript("Não perca minha mensagem"));
    expect(draft).toHaveBeenCalledWith("Não perca minha mensagem");
    expect(mock.control).toHaveBeenCalledWith("voice-1", "resume");
  });
  it("answers questions by voice but never treats spoken text as tool approval", async () => {
    mock.session = voiceSession(); mock.active = true;
    const question = { turnId: "t", toolId: "q", questions: [{ id: "style", question: "Qual estilo?", options: [{ label: "Clássico" }, { label: "Moderno" }] }, { id: "size", question: "Qual tamanho?", options: [{ label: "Pequeno" }, { label: "Grande" }] }] };
    const answer = vi.fn().mockResolvedValue(true), pause = vi.fn().mockResolvedValue(true), draft = vi.fn(), onMessage = vi.fn();
    const { rerender } = render(<VoiceControls target={voiceTarget} snapshot={{ ...emptyChat(), pendingQuestion: question }} onDictation={draft} onMessage={onMessage} onAnswerQuestion={answer} onPauseQuestion={pause} />);
    expect(pause).toHaveBeenCalledWith(question);
    await emit(transcript("Opção dois"));
    expect(answer).not.toHaveBeenCalled();
    expect(mock.control).toHaveBeenLastCalledWith("voice-1", "speak", expect.stringContaining("Qual tamanho? Opções: 1, Pequeno. 2, Grande"));
    await emit(transcript("Primeira", "call", 2));
    expect(answer).toHaveBeenCalledWith(question, { cancelled: false, answers: [{ id: "style", value: "Moderno", selectedLabel: "Moderno" }, { id: "size", value: "Pequeno", selectedLabel: "Pequeno" }] });
    const pendingApproval = { policy: null, tool: { id: "permission", name: "run", args: {}, status: "pending" as const, output: "", durationMs: 0 } };
    rerender(<VoiceControls target={voiceTarget} snapshot={{ ...emptyChat(), pendingApproval }} onDictation={draft} onMessage={onMessage} />);
    await emit(transcript("Pode aprovar", "call", 3));
    expect(draft).toHaveBeenCalledWith("Pode aprovar"); expect(onMessage).not.toHaveBeenCalled();
    expect(mock.control).toHaveBeenLastCalledWith("voice-1", "speak", expect.stringContaining("aprovação"));
  });
  it("uses explicit voice confirmation for project handoff and closes only the owned session", async () => {
    mock.session = voiceSession(); mock.active = true;
    const confirm = vi.fn().mockResolvedValue(true);
    const { unmount } = render(<VoiceControls target={voiceTarget} snapshot={emptyChat()} proposal={{ id: "p", projectName: "Movarte" }} onConfirmProject={confirm} onDictation={vi.fn()} onMessage={vi.fn()} />);
    await emit(transcript("Confirmo")); expect(confirm).toHaveBeenCalledExactlyOnceWith(true);
    unmount(); expect(mock.control).toHaveBeenCalledWith("voice-1", "end");
  });
  it("resumes the general call after declining a project and preserves a retargeted call on unmount", async () => {
    mock.session = voiceSession(); mock.active = true;
    const confirm = vi.fn().mockResolvedValue(true);
    const { unmount, rerender } = render(<VoiceControls target={voiceTarget} snapshot={emptyChat()} proposal={{ id: "p", projectName: "Movarte" }} onConfirmProject={confirm} onDictation={vi.fn()} onMessage={vi.fn()} />);
    await emit(transcript("Agora não")); expect(confirm).toHaveBeenCalledWith(false);
    expect(mock.control).toHaveBeenLastCalledWith("voice-1", "resume");
    mock.session = voiceSession({ target: "companion:new-project" });
    rerender(<VoiceControls target={voiceTarget} onDictation={vi.fn()} onMessage={vi.fn()} />);
    mock.control.mockClear(); unmount(); expect(mock.control).not.toHaveBeenCalled();
  });
  it("cancels preparation without opening a microphone and exposes call controls", () => {
    mock.session = voiceSession({ mode: "dictation", phase: "preparing" }); mock.active = true;
    const { rerender } = render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} onMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Ligar para o Jarvis" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Concluir ditado" })); expect(mock.control).toHaveBeenCalledWith("voice-1", "end");
    mock.session = voiceSession({ phase: "speaking", transcript: "Olá!", speaker: "jarvis", level: .7 });
    rerender(<VoiceSessionPanel target={voiceTarget} large />);
    expect(screen.getByText("Jarvis: Olá!")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Interromper fala e ouvir" })); expect(mock.control).toHaveBeenCalledWith("voice-1", "interrupt");
    fireEvent.click(screen.getByRole("button", { name: "Encerrar voz" })); expect(mock.control).toHaveBeenCalledWith("voice-1", "end");
  });
  it("opens setup when the local model is absent instead of secretly downloading it", () => {
    mock.settings = voiceSettings({ models: [] });
    render(<VoiceControls target={voiceTarget} onDictation={vi.fn()} onMessage={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Ligar para o Jarvis" }));
    expect(screen.getByRole("dialog")).toBeVisible(); expect(mock.start).not.toHaveBeenCalled();
  });
});
