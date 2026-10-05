import { act, renderHook, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CompanionItem } from "@/core/companion";
import type { VoiceSession, VoiceSettings } from "@/core/voice";
import { voiceSession, voiceSettings } from "@/test/voice-fixtures";
import { companionSpeechText, useCompanionSpeech } from "./use-companion-speech";
import { useCompanionNotices, useCompanionNoticeLifetime } from "./use-companion-notices";

const voice = vi.hoisted(() => ({ active: false, session: null as VoiceSession | null, settings: null as VoiceSettings | null, start: vi.fn(), control: vi.fn() }));
vi.mock("@/hooks/use-voice", () => ({ useVoice: () => voice }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const item = (status: CompanionItem["status"], id = "chat"): CompanionItem => ({
  conversationId: id, agentId: null, projectId: "project", projectName: "Portal", global: false,
  title: "Revisão da funcionalidade Sienge", role: "builder", status, activity: "", durationMs: 0, activeSince: null,
  updatedAt: 1, revision: 1, requiresConversation: false, attentionId: `${id}/${status}`, acknowledged: false, tasks: [],
});
let speechChanged: (() => void) | undefined;

function useNotifiedSpeech(items: CompanionItem[]) {
  const notices = useCompanionNotices();
  const { sync } = notices;
  useEffect(() => { sync(items); }, [items, sync]);
  const speech = useCompanionSpeech(items, true, true, notices.notice);
  useCompanionNoticeLifetime(notices.notice?.id ?? null, speech.pending || speech.playing, notices.dismiss);
  return { ...speech, notice: speech.pending ? null : notices.notice, clear: notices.clear };
}

describe("Jarvito spoken notifications", () => {
  beforeEach(() => {
    vi.clearAllMocks(); voice.active = false; voice.session = null; voice.settings = voiceSettings(); voice.control.mockResolvedValue(undefined);
    vi.spyOn(Math, "random").mockReturnValue(0);
    speechChanged = undefined;
    vi.mocked(listen).mockImplementation(async (name, callback) => {
      if (name === "companion:speech_changed") speechChanged = () => callback({ event: name, id: 1, payload: true });
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (command, args) => command === "set_companion_speech" ? (args as { enabled: boolean }).enabled
      : command === "get_companion_speech_volume" ? 1 : command === "set_companion_speech_volume" ? (args as { volume: number }).volume : true);
    voice.start.mockImplementation(async () => {
      voice.active = true; voice.session = voiceSession({ mode: "announcement", phase: "preparing", owner: "companion", target: "companion-notice" });
      return voice.session;
    });
  });
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

  it("names the actual task, varies its phrases, and stays silent during reconnection", () => {
    for (const status of ["completed", "failed", "waiting"] as const) {
      const phrases = Array.from({ length: 6 }, (_, i) => companionSpeechText(item(status), i));
      expect(new Set(phrases).size).toBe(6);
      for (const phrase of phrases) expect(phrase).toContain("Revisão da funcionalidade Sienge");
    }
    expect(companionSpeechText(item("reconnecting"), 0)).toBeNull();
    expect(companionSpeechText({ ...item("completed"), acknowledged: true }, 0)).toBeNull();
    expect(companionSpeechText({ ...item("completed"), global: true, title: "Jarvito" }, 0)).toContain("nossa conversa");
    expect(companionSpeechText({ ...item("completed"), agentId: "designer" }, 0)).toBeNull();
    expect(companionSpeechText({ ...item("waiting"), agentId: "designer" }, 0)).toContain("Revisão da funcionalidade Sienge");
  });

  it("speaks once when the root finishes after multiple worker completions", async () => {
    const root = item("running");
    const designer = { ...root, agentId: "designer" };
    const builder = { ...root, agentId: "builder" };
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [root, designer, builder] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    const finishedDesigner = { ...designer, status: "completed" as const, attentionId: "designer/completed" };
    const finishedBuilder = { ...builder, status: "completed" as const, attentionId: "builder/completed" };
    rerender({ items: [root, finishedDesigner, builder] });
    rerender({ items: [root, finishedDesigner, finishedBuilder] });
    expect(voice.start).not.toHaveBeenCalled();
    expect(result.current.notice).toBeNull();
    rerender({ items: [item("completed"), finishedDesigner, finishedBuilder] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
  });

  it("does not replay loaded history, speaks a new result once and queues the next result", async () => {
    const old = { ...item("completed", "old"), acknowledged: true }, current = item("running");
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [old, current] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    expect(voice.start).not.toHaveBeenCalled();
    rerender({ items: [old, item("completed"), item("failed", "next")] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledTimes(1));
    expect(voice.start.mock.calls[0]).toEqual(["companion-notice", "announcement", expect.stringContaining(current.title)]);
    rerender({ items: [old, { ...item("completed"), revision: 2 }, item("failed", "next")] });
    expect(voice.start).toHaveBeenCalledTimes(1);
    voice.active = false; voice.session = null;
    rerender({ items: [old, { ...item("completed"), acknowledged: true }, item("failed", "next")] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledTimes(2));
  });

  it("keeps dictation in control of audio and never narrates its old notices afterward", async () => {
    voice.active = true; voice.session = voiceSession({ mode: "dictation" });
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    rerender({ items: [item("completed")] });
    voice.active = false; voice.session = null;
    rerender({ items: [item("completed")] });
    expect(voice.start).not.toHaveBeenCalled();
  });

  it("plays available bundled variants without local voice models and preserves card synchronization", async () => {
    const disabled = voiceSettings();
    voice.settings = { ...disabled, config: { ...disabled.config, enabled: false }, speechReady: false,
      announcementClips: ["failed-1", "completed-6", "completed-2"] };
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    const completed = [item("completed")];
    rerender({ items: completed });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    expect(voice.start).toHaveBeenCalledWith("companion-notice", "announcement", expect.stringContaining(completed[0].title), "completed-2");
    expect(result.current.notice).toBeNull();
    voice.session = voiceSession({ mode: "announcement", phase: "speaking" });
    rerender({ items: completed });
    expect(result.current.notice?.item.status).toBe("completed");
    voice.active = false; voice.session = null;
    rerender({ items: [item("running", "next")] });
    rerender({ items: [item("completed", "next")] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledTimes(2));
    expect(voice.start.mock.calls[1]?.[3]).toBe("completed-6");
  });

  it("rotates through all eight available completed recordings over consecutive notifications", async () => {
    const clips = Array.from({ length: 8 }, (_, index) => `completed-${index + 1}`);
    voice.settings = voiceSettings({ announcementClips: [...clips].reverse() });
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    for (let index = 0; index < clips.length; index += 1) {
      const completed = item("completed", `completed-${index}`);
      rerender({ items: [completed] });
      await waitFor(() => expect(voice.start).toHaveBeenCalledTimes(index + 1));
      expect(voice.start.mock.calls[index]?.[3]).toBe(clips[index]);
      voice.active = false; voice.session = null;
      act(() => result.current.clear());
    }
    expect(voice.start.mock.calls.map(call => call[3])).toEqual(clips);
  });

  it.each(["completed-7", "completed-8"])("plays %s when it is the only recording and local synthesis is disabled", async clip => {
    const disabled = voiceSettings({ speechReady: false, announcementClips: [clip] });
    voice.settings = { ...disabled, config: { ...disabled.config, enabled: false } };
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    rerender({ items: [item("completed")] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    expect(voice.start).toHaveBeenCalledWith("companion-notice", "announcement", expect.stringContaining("Sienge"), clip);
  });

  it("uses local synthesis for categories without a clip and does not use an unrelated clip", async () => {
    voice.settings = voiceSettings({ announcementClips: ["failed-1", "completed-9", "../completed-1"] });
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    rerender({ items: [item("completed")] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    expect(voice.start.mock.calls[0]).toEqual(["companion-notice", "announcement", expect.stringContaining("Sienge")]);
  });

  it.each(["question", "approval"] as const)("selects a %s clip for the corresponding attention event", async kind => {
    voice.settings = voiceSettings({ announcementClips: ["question-1", "approval-1"] });
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    const waiting: CompanionItem = { ...item("waiting"), pendingQuestion: kind === "question"
      ? { turnId: "turn", toolId: "tool", questions: [{ id: "question", question: "Qual opção?", options: [] }] } : null };
    rerender({ items: [waiting] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    expect(voice.start.mock.calls[0]?.[3]).toBe(`${kind}-1`);
  });

  it("persists speech mute independently and keeps its old value when saving fails", async () => {
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.enabled).toBe(true));
    await act(async () => { await result.current.toggle(); });
    expect(invoke).toHaveBeenCalledWith("set_companion_speech", { enabled: false });
    expect(invoke).not.toHaveBeenCalledWith("set_companion_sound", expect.anything());
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Disk full"));
    await act(async () => { await expect(result.current.toggle()).rejects.toThrow("Disk full"); });
    expect(result.current.enabled).toBe(false);
    rerender({ items: [item("failed")] });
    expect(voice.start).not.toHaveBeenCalled();
  });

  it("restores voice volume and refreshes both preferences when another window changes them", async () => {
    let volume = 0.35, enabled = true;
    vi.mocked(invoke).mockImplementation(async command => command === "get_companion_speech_volume" ? volume : enabled);
    const { result } = renderHook(() => useCompanionSpeech([], true));
    await waitFor(() => expect(result.current.volumeReady).toBe(true));
    expect(result.current.volume).toBe(0.35);
    expect(result.current.enabled).toBe(true);
    volume = 0.72; enabled = false;
    act(() => speechChanged?.());
    await waitFor(() => expect(result.current.volume).toBe(0.72));
    expect(result.current.enabled).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith("set_companion_speech_volume", expect.anything());
  });

  it("persists voice volume without changing mute or interrupting an announcement", async () => {
    voice.active = true; voice.session = voiceSession({ mode: "announcement", phase: "speaking" });
    const original = vi.mocked(invoke).getMockImplementation();
    vi.mocked(invoke).mockImplementation(async (command, args) => command === "set_companion_speech_volume"
      ? Math.fround((args as { volume: number }).volume) : original?.(command, args));
    const { result } = renderHook(() => useCompanionSpeech([], true));
    await waitFor(() => expect(result.current.volumeReady).toBe(true));
    await act(async () => { await result.current.setVolume(0.65); });
    expect(invoke).toHaveBeenCalledWith("set_companion_speech_volume", { volume: 0.65 });
    expect(result.current.volume).toBe(Math.fround(0.65));
    expect(result.current.enabled).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("set_companion_speech", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("set_companion_sound", expect.anything());
    expect(voice.control).not.toHaveBeenCalled();
  });

  it("keeps the last saved volume and mute when saving volume fails", async () => {
    const { result } = renderHook(() => useCompanionSpeech([], true));
    await waitFor(() => expect(result.current.volumeReady).toBe(true));
    await act(async () => { await result.current.setVolume(0.4); });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Disk full"));
    await act(async () => { await expect(result.current.setVolume(0.1)).rejects.toThrow("Disk full"); });
    expect(result.current.volume).toBe(0.4);
    expect(result.current.enabled).toBe(true);
    expect(result.current.volumeSaving).toBe(false);
    expect(result.current.volumeError).toBe("Não foi possível salvar o volume da voz.");
  });

  it("keeps speech mute available while volume loads or fails to load", async () => {
    let rejectVolume: ((cause: Error) => void) | undefined;
    const original = vi.mocked(invoke).getMockImplementation();
    vi.mocked(invoke).mockImplementation(async (command, args) => command === "get_companion_speech_volume"
      ? new Promise<never>((_resolve, reject) => { rejectVolume = reject; }) : original?.(command, args));
    const { result } = renderHook(() => useCompanionSpeech([], true));
    await waitFor(() => expect(result.current.ready).toBe(true));
    expect(result.current.volumeReady).toBe(false);
    await act(async () => { await result.current.toggle(); });
    expect(result.current.enabled).toBe(false);
    await act(async () => { rejectVolume?.(new Error("Unavailable")); });
    expect(result.current.volumeReady).toBe(false);
    expect(result.current.volumeError).toBe("Não foi possível carregar o volume da voz.");
  });

  it("waits for prepared audio, keeps its card throughout playback and then grants 30 seconds", async () => {
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    const completed = [item("completed")];
    rerender({ items: completed });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    expect(result.current.notice).toBeNull();
    vi.useFakeTimers();
    await act(async () => { await vi.advanceTimersByTimeAsync(45_000); });
    expect(result.current.notice).toBeNull();
    voice.session = voiceSession({ mode: "announcement", phase: "speaking" });
    rerender({ items: completed });
    await act(async () => {});
    expect(result.current.notice?.item.title).toBe(completed[0].title);
    await act(async () => { await vi.advanceTimersByTimeAsync(40_000); });
    expect(result.current.notice?.item.title).toBe(completed[0].title);
    voice.session = voiceSession({ mode: "announcement", phase: "synthesizing" });
    rerender({ items: completed });
    expect(result.current.notice?.item.title).toBe(completed[0].title);
    voice.active = false; voice.session = voiceSession({ mode: "announcement", phase: "idle" });
    rerender({ items: completed });
    await act(async () => {});
    await act(async () => { await vi.advanceTimersByTimeAsync(29_999); });
    expect(result.current.notice).not.toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(result.current.notice).toBeNull();
    expect(voice.start).toHaveBeenCalledOnce();
  });

  it("cancels a dismissed announcement even if its start response arrives later", async () => {
    let resolve: ((session: VoiceSession) => void) | undefined;
    voice.start.mockImplementationOnce(() => new Promise<VoiceSession>(done => { resolve = done; }));
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    rerender({ items: [item("completed")] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    act(() => result.current.clear());
    await act(async () => { resolve?.(voiceSession({ id: "late-announcement", mode: "announcement" })); });
    expect(voice.control).toHaveBeenCalledWith("late-announcement", "end");
    expect(result.current.notice).toBeNull();
  });

  it("shows the card without retrying when audio preparation fails", async () => {
    voice.start.mockRejectedValueOnce(new Error("Speech unavailable"));
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    rerender({ items: [item("failed")] });
    await waitFor(() => expect(result.current.notice?.item.status).toBe("failed"));
    expect(voice.start).toHaveBeenCalledOnce();
  });

  it.each(["idle", "error"] as const)("releases the card and queue when %s arrives before the start reply", async phase => {
    let resolve: ((session: VoiceSession) => void) | undefined;
    voice.start.mockImplementationOnce(() => new Promise<VoiceSession>(done => { resolve = done; }));
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    const items = [item("completed"), { ...item("failed", "next"), updatedAt: 2 }];
    rerender({ items });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    vi.useFakeTimers();
    voice.session = voiceSession({ id: "finished-before-reply", mode: "announcement", phase });
    rerender({ items });
    await act(async () => { resolve?.(voiceSession({ id: "finished-before-reply", mode: "announcement", phase: "preparing" })); });
    expect(result.current.notice?.item.conversationId).toBe("chat");
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(voice.start).toHaveBeenCalledTimes(2);
  });

  it("silencing Jarvito cancels only the current announcement and releases its visual notice", async () => {
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    rerender({ items: [item("completed")] });
    await waitFor(() => expect(voice.start).toHaveBeenCalledOnce());
    await act(async () => { await result.current.toggle(); });
    expect(voice.control).toHaveBeenCalledWith("voice-1", "end");
    expect(result.current.notice?.item.status).toBe("completed");
  });
});
