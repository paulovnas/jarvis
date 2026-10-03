import { describe, expect, it } from "vitest";
import { voiceSession, voiceSettings } from "@/test/voice-fixtures";
import { voiceActive, voiceConfigSchema, voiceSessionSchema, voiceSettingsSchema, voiceTranscriptSchema } from "./voice";

describe("voice contract", () => {
  it("accepts dictation and speech playback modes while rejecting removed call sessions and transcripts", () => {
    for (const mode of ["dictation", "test", "announcement"] as const) expect(voiceSessionSchema.safeParse(voiceSession({ mode })).success).toBe(true);
    expect(voiceSessionSchema.safeParse({ ...voiceSession(), mode: "call" }).success).toBe(false);
    const transcript = { sessionId: "voice-1", target: "chat:1", sequence: 1, text: "Meu texto", mode: "dictation" };
    expect(voiceTranscriptSchema.safeParse(transcript).success).toBe(true);
    expect(voiceTranscriptSchema.safeParse({ ...transcript, mode: "call" }).success).toBe(false);
  });
  it("keeps ownership during shutdown and rejects invalid calibration", () => {
    expect(voiceActive(voiceSession({ phase: "closing" }))).toBe(true);
    expect(voiceActive(voiceSession({ phase: "error" }))).toBe(false);
    expect(voiceConfigSchema.safeParse({ enabled: true, microphone: null, speaker: null, model: "small", voice: "pm_alex", speed: NaN, silenceMs: 50 }).success).toBe(false);
  });
  it("loads optional recorded announcement clips without breaking existing settings", () => {
    const previous = { ...voiceSettings(), announcementClips: undefined };
    expect(voiceSettingsSchema.parse(previous).announcementClips).toEqual([]);
    expect(voiceSettingsSchema.parse({ ...previous, announcementClips: ["completed-1", "question-2"] }).announcementClips).toEqual(["completed-1", "question-2"]);
  });
});
