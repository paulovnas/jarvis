import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { CompanionItem } from "@/core/companion";
import { useCompanionSounds } from "./use-companion-sounds";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const item = (status: CompanionItem["status"], id = "chat"): CompanionItem => ({
  conversationId: id, agentId: null, projectId: "project", projectName: "Portal", title: id, role: "builder", status,
  global: false, activity: "", durationMs: 1000, activeSince: null, updatedAt: 1, requiresConversation: false, attentionId: `${id}/${status}`, acknowledged: false,
  tasks: [],
});
const start = vi.fn();
const stop = vi.fn();
const suspend = vi.fn().mockResolvedValue(undefined);
const close = vi.fn().mockResolvedValue(undefined);
const parameter = () => ({ setValueAtTime: vi.fn(), linearRampToValueAtTime: vi.fn(), exponentialRampToValueAtTime: vi.fn() });
class Audio {
  state = "running";
  currentTime = 0;
  destination = {};
  suspend = suspend;
  close = close;
  resume = vi.fn().mockResolvedValue(undefined);
  createOscillator() { return { type: "sine", frequency: parameter(), connect: vi.fn(), disconnect: vi.fn(), start, stop, onended: null }; }
  createGain() { return { gain: parameter(), connect: vi.fn(), disconnect: vi.fn() }; }
}

describe("Jarvito native lifecycle sound cues", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("AudioContext", Audio);
    vi.mocked(invoke).mockImplementation(async (command, args) => command === "set_companion_sound" ? (args as { enabled: boolean }).enabled : true);
  });
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("keeps loaded history silent and plays a new completion once across revisions and acknowledgments", async () => {
    const { result, rerender, unmount } = renderHook(({ items, loaded }) => useCompanionSounds(items, loaded), { initialProps: { items: [] as CompanionItem[], loaded: false } });
    await waitFor(() => expect(result.current.ready).toBe(true));
    const done = item("completed");
    rerender({ items: [done], loaded: true });
    expect(start).not.toHaveBeenCalled();
    const next = { ...done, conversationId: "next", attentionId: "next/completed" };
    rerender({ items: [done, next], loaded: true });
    expect(start).toHaveBeenCalledTimes(3);
    rerender({ items: [done, { ...next, revision: 2, updatedAt: 20 }], loaded: true });
    rerender({ items: [done, { ...next, acknowledged: true }], loaded: true });
    expect(start).toHaveBeenCalledTimes(3);
    unmount();
    expect(close).toHaveBeenCalledOnce();
  });

  it("responds to root and child state transitions without playing per-token activity updates", async () => {
    let clock = 0;
    vi.spyOn(performance, "now").mockImplementation(() => clock);
    const root = item("running");
    const { result, rerender } = renderHook(({ items }) => useCompanionSounds(items, true), { initialProps: { items: [root] } });
    await waitFor(() => expect(result.current.enabled).toBe(true));
    const child = { ...item("running"), agentId: "designer" };
    clock += 1000;
    rerender({ items: [root, child] });
    expect(start).toHaveBeenCalledTimes(1);
    rerender({ items: [{ ...root, activity: "Novo texto" }, { ...child, updatedAt: 5 }] });
    expect(start).toHaveBeenCalledTimes(1);
    clock += 1000;
    rerender({ items: [root, { ...child, status: "waiting", attentionId: "question" }] });
    expect(start).toHaveBeenCalledTimes(3);
    clock += 1000;
    rerender({ items: [root, { ...child, status: "failed", attentionId: "failure" }] });
    expect(start).toHaveBeenCalledTimes(5);
  });

  it("plays the finish cue only for the whole request after workers hand off their results", async () => {
    const root = item("running");
    const child = { ...root, agentId: "designer" };
    const { result, rerender } = renderHook(({ items }) => useCompanionSounds(items, true), { initialProps: { items: [root, child] } });
    await waitFor(() => expect(result.current.enabled).toBe(true));
    const completedChild = { ...child, status: "completed" as const, attentionId: "designer/completed" };
    rerender({ items: [root, completedChild] });
    expect(start).not.toHaveBeenCalled();
    rerender({ items: [item("completed"), completedChild] });
    expect(start).toHaveBeenCalledTimes(3);
  });

  it("persists mute before changing state, stays muted after failed saves, and keeps hidden transitions silent", async () => {
    const { result, rerender } = renderHook(({ items, visible }) => useCompanionSounds(items, true, visible), { initialProps: { items: [] as CompanionItem[], visible: false } });
    await waitFor(() => expect(result.current.enabled).toBe(true));
    rerender({ items: [item("completed")], visible: false });
    expect(start).not.toHaveBeenCalled();
    await act(async () => { await result.current.toggle(); });
    expect(invoke).toHaveBeenCalledWith("set_companion_sound", { enabled: false });
    expect(result.current.enabled).toBe(false);
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Disk full"));
    await act(async () => { await expect(result.current.toggle()).rejects.toThrow("Disk full"); });
    expect(result.current.enabled).toBe(false);
    rerender({ items: [item("failed")], visible: true });
    act(() => result.current.play("open"));
    expect(start).not.toHaveBeenCalled();
  });

  it("allows interaction when WebAudio is unavailable or the preference cannot be loaded", async () => {
    vi.stubGlobal("AudioContext", undefined);
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Unavailable"));
    const { result } = renderHook(() => useCompanionSounds([], true));
    await waitFor(() => expect(result.current.ready).toBe(true));
    await act(async () => { await result.current.toggle(); });
    expect(result.current.enabled).toBe(true);
    expect(() => act(() => result.current.play("open"))).not.toThrow();
  });

  it("waits for the saved preference before allowing a toggle", async () => {
    let finish: (enabled: boolean) => void = () => {};
    vi.mocked(invoke).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const { result } = renderHook(() => useCompanionSounds([], true));
    await act(async () => { await result.current.toggle(); });
    expect(invoke).not.toHaveBeenCalledWith("set_companion_sound", expect.anything());
    await act(async () => { finish(true); });
    expect(result.current.ready).toBe(true);
    await act(async () => { await result.current.toggle(); });
    expect(result.current.enabled).toBe(false);
  });

  it("plays the triple-poke reaction even when it immediately follows a poke", async () => {
    vi.spyOn(performance, "now").mockReturnValue(0);
    const { result } = renderHook(() => useCompanionSounds([], true));
    await waitFor(() => expect(result.current.enabled).toBe(true));
    act(() => result.current.play("poke"));
    expect(start).toHaveBeenCalledTimes(1);
    act(() => result.current.play("dizzy"));
    expect(start).toHaveBeenCalledTimes(4);
  });
});
