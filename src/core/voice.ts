import { z } from "zod";

export const voiceConfigSchema = z.object({
  enabled: z.boolean(), microphone: z.string().nullable(), speaker: z.string().nullable(),
  model: z.enum(["small", "tiny"]), voice: z.enum(["pf_dora", "pm_alex", "pm_santa"]),
  speed: z.number().min(0.75).max(1.5), silenceMs: z.number().int().min(400).max(1800),
});
export const voiceSessionSchema = z.object({
  id: z.string().nullable(), target: z.string().nullable(), owner: z.string().nullable(),
  mode: z.enum(["dictation", "test", "announcement"]).nullable(),
  phase: z.enum(["idle", "preparing", "listening", "transcribing", "synthesizing", "speaking", "closing", "error"]),
  level: z.number().min(0).max(1), transcript: z.string(), error: z.string().nullable(), revision: z.number().int().nonnegative(),
  startedAt: z.number().nonnegative().nullable(), speaker: z.enum(["user", "jarvis"]).nullable(),
});
export const voiceDownloadSchema = z.object({ model: z.string(), received: z.number().nonnegative(), total: z.number().positive() });
export const voiceSettingsSchema = z.object({
  config: voiceConfigSchema,
  microphones: z.array(z.object({ id: z.string(), name: z.string() })),
  speakers: z.array(z.object({ id: z.string(), name: z.string() })),
  models: z.array(z.object({ id: z.string(), name: z.string(), bytes: z.number().positive(), installed: z.boolean() })),
  speechReady: z.boolean(), speechError: z.string().nullable(), announcementClips: z.array(z.string()).default([]), download: voiceDownloadSchema.nullable(), session: voiceSessionSchema,
});
export const voiceTranscriptSchema = z.object({ sessionId: z.string(), target: z.string(), sequence: z.number().int().positive(), text: z.string().min(1), mode: z.literal("dictation") });
export type VoiceConfig = z.infer<typeof voiceConfigSchema>;
export type VoiceSession = z.infer<typeof voiceSessionSchema>;
export type VoiceSettings = z.infer<typeof voiceSettingsSchema>;
export const voiceActive = (session: VoiceSession | null | undefined) => !!session?.id && session.phase !== "idle" && session.phase !== "error";
export const voicePhaseLabels: Record<VoiceSession["phase"], string> = {
  idle: "Voz encerrada", preparing: "Preparando voz local…", listening: "Estou ouvindo", transcribing: "Entendendo sua fala…",
  synthesizing: "Preparando áudio…", speaking: "Falando", closing: "Encerrando voz…", error: "Voz interrompida",
};
