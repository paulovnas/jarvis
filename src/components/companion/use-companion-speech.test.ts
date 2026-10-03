import { act, renderHook, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CompanionItem } from "@/core/companion";
import type { VoiceSession } from "@/core/voice";
import { voiceSession, voiceSettings } from "@/test/voice-fixtures";
import { companionSpeechText, useCompanionSpeech } from "./use-companion-speech";
import { useCompanionNotices, useCompanionNoticeLifetime } from "./use-companion-notices";

const voice = vi.hoisted(() => ({ active: false, session: null as VoiceSession | null, start: vi.fn(), control: vi.fn() }));
vi.mock("@/hooks/use-voice", () => ({ useVoice: () => ({ ...voice, settings: voiceSettings() }) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const item = (status: CompanionItem["status"], id = "chat"): CompanionItem => ({
  conversationId: id, agentId: null, projectId: "project", projectName: "Portal", global: false,
  title: "Revisão da funcionalidade Sienge", role: "builder", status, activity: "", durationMs: 0, activeSince: null,
  updatedAt: 1, revision: 1, requiresConversation: false, attentionId: `${id}/${status}`, acknowledged: false, tasks: [],
});

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
    vi.clearAllMocks(); voice.active = false; voice.session = null; voice.control.mockResolvedValue(undefined);
    vi.mocked(invoke).mockImplementation(async (command, args) => command === "set_companion_speech" ? (args as { enabled: boolean }).enabled : true);
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

  it.each(["call", "dictation"] as const)("keeps %s in control of audio and never narrates its old notices afterward", async mode => {
    voice.active = true; voice.session = voiceSession({ mode });
    const { result, rerender } = renderHook(({ items }) => useNotifiedSpeech(items), { initialProps: { items: [item("running")] } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    rerender({ items: [item("completed")] });
    voice.active = false; voice.session = null;
    rerender({ items: [item("completed")] });
    expect(voice.start).not.toHaveBeenCalled();
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
