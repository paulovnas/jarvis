import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { idleVoice, voiceSession, voiceSettings, voiceTarget } from "@/test/voice-fixtures";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
afterEach(() => vi.restoreAllMocks());
describe("shared native voice state", () => {
  it("shares subscriptions, rejects old revisions and ignores malformed audio events", async () => {
    vi.resetModules();
    const events = new Map<string, (payload: unknown) => void>(), stopped = vi.fn();
    vi.mocked(listen).mockImplementation(async (name, callback) => { events.set(String(name), payload => callback({ event: String(name), id: 1, payload })); return stopped; });
    let complete: (value: unknown) => void = () => {};
    vi.mocked(invoke).mockImplementation(async command => command === "get_voice_settings" ? new Promise(resolve => { complete = resolve; }) : undefined);
    const { useVoice } = await import("./use-voice");
    const first = renderHook(useVoice), second = renderHook(useVoice);
    await waitFor(() => expect(events.size).toBe(3));
    await act(async () => { events.get("voice:state")?.(voiceSession({ revision: 9 })); complete(voiceSettings({ session: { ...idleVoice, revision: 1 } })); });
    expect(first.result.current.session?.revision).toBe(9); expect(second.result.current.active).toBe(true);
    await act(async () => { events.get("voice:state")?.({ id: "invalid" }); events.get("voice:state")?.(voiceSession({ revision: 8, phase: "idle" })); });
    expect(first.result.current.session?.phase).toBe("listening");
    first.unmount(); expect(stopped).not.toHaveBeenCalled(); second.unmount();
    await waitFor(() => expect(stopped).toHaveBeenCalledTimes(3));
  });
  it("retargets the native call before the old conversation unmounts", async () => {
    vi.resetModules(); vi.mocked(listen).mockImplementation(async () => () => {});
    const nextTarget = "companion:abcdef0123456789abcdef0123456789";
    vi.mocked(invoke).mockImplementation(async command => command === "get_voice_settings" ? voiceSettings() : command === "start_voice_session" ? voiceSession() : command === "get_voice_session" ? voiceSession({ target: nextTarget, owner: "companion", revision: 3 }) : undefined);
    const { useVoice } = await import("./use-voice"); const { result, unmount } = renderHook(useVoice);
    await waitFor(() => expect(result.current.settings).not.toBeNull());
    await act(async () => { await result.current.start(voiceTarget, "call"); await result.current.control("voice-1", "retarget", nextTarget); });
    expect(result.current.session?.target).toBe(nextTarget);
    expect(invoke).toHaveBeenCalledWith("control_voice_session", { sessionId: "voice-1", action: "retarget", text: nextTarget });
    unmount();
  });
});
