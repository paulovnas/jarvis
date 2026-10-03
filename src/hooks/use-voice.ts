import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { libraryError } from "@/core/library";
import { voiceActive, voiceDownloadSchema, voiceSessionSchema, voiceSettingsSchema, type VoiceConfig, type VoiceSession, type VoiceSettings } from "@/core/voice";

type Snapshot = { settings: VoiceSettings | null; session: VoiceSession | null; loading: boolean; error: string | null };
let snapshot: Snapshot = { settings: null, session: null, loading: false, error: null };
let flight: Promise<void> | null = null;
let subscriptions: Promise<UnlistenFn[]> | null = null;
let stoppedDictation: string | null = null;
const listeners = new Set<() => void>();
const read = () => snapshot;
function update(next: Snapshot) { snapshot = next; listeners.forEach(listener => listener()); }
function applySession(next: VoiceSession) {
  if (snapshot.session && snapshot.session.revision > next.revision) return;
  update({ ...snapshot, session: next });
}
async function refresh() {
  if (flight) return flight;
  const installed = subscriptions;
  update({ ...snapshot, loading: true });
  flight = invoke("get_voice_settings").then(value => {
    const settings = voiceSettingsSchema.parse(value);
    update({ ...snapshot, settings, loading: false, error: installed && installed === subscriptions ? null : snapshot.error }); applySession(settings.session);
  }).catch(cause => update({ ...snapshot, loading: false, error: typeof cause === "string" ? cause : libraryError(cause, "Não foi possível consultar o Jarvis Voice.") }))
    .finally(() => { flight = null; });
  return flight;
}
function ensureSubscriptions(): Promise<UnlistenFn[]> {
  if (subscriptions) return subscriptions;
  if (!listeners.size) return Promise.reject("A interface de voz foi fechada. Tente novamente.");
  const pending = Promise.allSettled([
    listen("voice:state", event => { const value = voiceSessionSchema.safeParse(event.payload); if (value.success) applySession(value.data); }),
    listen("voice:download", event => { const value = voiceDownloadSchema.safeParse(event.payload); if (value.success && snapshot.settings) update({ ...snapshot, settings: { ...snapshot.settings, download: value.data } }); }),
    listen("voice:changed", () => { void (flight ?? Promise.resolve()).then(() => refresh()); }),
  ]).then(results => {
    const stops = results.flatMap(result => result.status === "fulfilled" ? [result.value] : []);
    if (subscriptions !== pending) {
      stops.forEach(stop => stop());
      throw "A interface de voz foi fechada. Tente novamente.";
    }
    if (results.some(result => result.status === "rejected")) {
      stops.forEach(stop => stop());
      subscriptions = null;
      const error = "Não foi possível acompanhar a sessão de voz. Tente novamente.";
      update({ ...snapshot, error });
      throw error;
    }
    update({ ...snapshot, error: null });
    return stops;
  });
  subscriptions = pending;
  return pending;
}
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  const pending = ensureSubscriptions();
  void pending.catch(() => {});
  void refresh();
  return () => {
    listeners.delete(listener);
    if (!listeners.size) { void subscriptions?.then(stops => stops.forEach(stop => stop())).catch(() => {}); subscriptions = null; }
  };
};

async function save(config: VoiceConfig) {
  const settings = voiceSettingsSchema.parse(await invoke("save_voice_settings", { config }));
  update({ ...snapshot, settings, error: null }); applySession(settings.session);
}
async function start(target: string, mode: "dictation" | "test" | "announcement", text?: string, clip?: string) {
  const pending = ensureSubscriptions();
  await pending;
  if (subscriptions !== pending || !listeners.size) throw "A interface de voz foi fechada. Tente novamente.";
  const session = voiceSessionSchema.parse(await invoke("start_voice_session", { target, mode, ...(text ? { text } : {}), ...(clip ? { clip } : {}) }));
  applySession(session); return session;
}
async function control(sessionId: string, action: "end" | "finish") {
  await invoke("control_voice_session", { sessionId, action });
}

export const acceptsDictationTranscript = (sessionId: string) => stoppedDictation !== sessionId;
export async function stopDictation(target?: string) {
  const session = snapshot.session;
  if (!target || session?.target !== target || session.mode !== "dictation" || !voiceActive(session) || !session.id || stoppedDictation === session.id) return;
  // A transcript can already be queued in the renderer when capture is cancelled.
  stoppedDictation = session.id;
  try { await control(session.id, "end"); }
  catch (cause) { if (stoppedDictation === session.id) stoppedDictation = null; throw cause; }
}

export function useVoice() {
  const state = useSyncExternalStore(subscribe, read, read);
  return { ...state, active: voiceActive(state.session), refresh, save, start, control };
}
