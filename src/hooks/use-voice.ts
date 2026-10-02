import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { libraryError } from "@/core/library";
import { voiceActive, voiceDownloadSchema, voiceSessionSchema, voiceSettingsSchema, type VoiceConfig, type VoiceSession, type VoiceSettings } from "@/core/voice";

type Snapshot = { settings: VoiceSettings | null; session: VoiceSession | null; loading: boolean; error: string | null };
let snapshot: Snapshot = { settings: null, session: null, loading: false, error: null };
let flight: Promise<void> | null = null;
let subscriptions: Promise<UnlistenFn[]> | null = null;
const listeners = new Set<() => void>();
const read = () => snapshot;
function update(next: Snapshot) { snapshot = next; listeners.forEach(listener => listener()); }
function applySession(next: VoiceSession) {
  if (snapshot.session && snapshot.session.revision > next.revision) return;
  update({ ...snapshot, session: next });
}
async function refresh() {
  if (flight) return flight;
  update({ ...snapshot, loading: true });
  flight = invoke("get_voice_settings").then(value => {
    const settings = voiceSettingsSchema.parse(value);
    update({ ...snapshot, settings, loading: false, error: null }); applySession(settings.session);
  }).catch(cause => update({ ...snapshot, loading: false, error: typeof cause === "string" ? cause : libraryError(cause, "Não foi possível consultar o Jarvis Voice.") }))
    .finally(() => { flight = null; });
  return flight;
}
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  if (!subscriptions) {
    subscriptions = Promise.all([
      listen("voice:state", event => { const value = voiceSessionSchema.safeParse(event.payload); if (value.success) applySession(value.data); }),
      listen("voice:download", event => { const value = voiceDownloadSchema.safeParse(event.payload); if (value.success && snapshot.settings) update({ ...snapshot, settings: { ...snapshot.settings, download: value.data } }); }),
      listen("voice:changed", () => { void (flight ?? Promise.resolve()).then(() => refresh()); }),
    ].map(result => result.catch(() => () => {})));
    void refresh();
  }
  return () => {
    listeners.delete(listener);
    if (!listeners.size) { void subscriptions?.then(stops => stops.forEach(stop => stop())); subscriptions = null; }
  };
};

async function save(config: VoiceConfig) {
  const settings = voiceSettingsSchema.parse(await invoke("save_voice_settings", { config }));
  update({ ...snapshot, settings, error: null }); applySession(settings.session);
}
async function start(target: string, mode: "dictation" | "call" | "test") {
  const session = voiceSessionSchema.parse(await invoke("start_voice_session", { target, mode }));
  applySession(session); return session;
}
async function control(sessionId: string, action: "end" | "finish" | "resume" | "interrupt" | "mute" | "unmute" | "speak" | "retarget", text?: string) {
  await invoke("control_voice_session", { sessionId, action, ...(text ? { text } : {}) });
  if (action === "retarget") applySession(voiceSessionSchema.parse(await invoke("get_voice_session")));
}

export function useVoice() {
  const state = useSyncExternalStore(subscribe, read, read);
  useEffect(() => { if (!snapshot.settings && !snapshot.loading && !snapshot.error) void refresh(); }, []);
  return { ...state, active: voiceActive(state.session), refresh, save, start, control };
}
