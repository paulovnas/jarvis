import type { VoiceSession, VoiceSettings } from "@/core/voice";

export const voiceTarget = "chat:0123456789abcdef0123456789abcdef";
export const idleVoice: VoiceSession = { id: null, target: null, owner: null, mode: null, phase: "idle", muted: false, level: 0, transcript: "", error: null, revision: 1, startedAt: null, speaker: null };
export function voiceSession(patch: Partial<VoiceSession> = {}): VoiceSession {
  return { ...idleVoice, id: "voice-1", target: voiceTarget, owner: "main", mode: "call", phase: "listening", startedAt: Date.now(), revision: 2, ...patch };
}
export function voiceSettings(patch: Partial<VoiceSettings> = {}): VoiceSettings {
  return { config: { enabled: true, microphone: null, speaker: null, model: "small", voice: "pm_alex", speed: 1, silenceMs: 650 }, microphones: [{ id: "mic-1", name: "Microfone USB" }], speakers: [{ id: "out-1", name: "Fones de ouvido" }], models: [{ id: "small", name: "Small · melhor precisão", bytes: 190085487, installed: true }, { id: "tiny", name: "Tiny · mais rápido", bytes: 32152673, installed: false }], speechReady: true, speechError: null, download: null, session: { ...idleVoice }, ...patch };
}
