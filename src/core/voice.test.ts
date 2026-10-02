import { describe, expect, it } from "vitest";
import { savedTurn } from "@/test/chat-fixtures";
import { voiceSession } from "@/test/voice-fixtures";
import { voiceActive, voiceConfigSchema, voiceQuestionAnswer, voiceReply } from "./voice";

describe("voice conversation contract", () => {
  it("uses only the final visible reply, preserving error and interrupted states", () => {
    const turn = savedTurn();
    turn.steps.push({ ...turn.steps[0], text: "Feito!", summary: "Private work" });
    expect(voiceReply(turn)).toBe("Feito!");
    expect(voiceReply({ ...turn, status: "error", error: { message: "Sem conexão", code: "network" } })).toBe("Sem conexão");
    expect(voiceReply({ ...turn, status: "cancelled" })).toContain("interrompida");
  });
  it("recognizes Portuguese option numbers and labels while preserving free text", () => {
    const question = { id: "q1", header: "Estilo", question: "Qual estilo?", options: [{ label: "Clássico", description: "Clássico" }, { label: "Moderno", description: "Moderno" }] };
    expect(voiceQuestionAnswer(question, "Opção duas.")).toEqual({ id: "q1", value: "Moderno", selectedLabel: "Moderno" });
    expect(voiceQuestionAnswer(question, "classico!").selectedLabel).toBe("Clássico");
    expect(voiceQuestionAnswer(question, "Prefiro um estilo industrial")).toEqual({ id: "q1", value: "Prefiro um estilo industrial" });
  });
  it("keeps ownership during shutdown and rejects invalid calibration", () => {
    expect(voiceActive(voiceSession({ phase: "closing" }))).toBe(true);
    expect(voiceActive(voiceSession({ phase: "error" }))).toBe(false);
    expect(voiceConfigSchema.safeParse({ enabled: true, microphone: null, speaker: null, model: "small", voice: "pm_alex", speed: NaN, silenceMs: 50 }).success).toBe(false);
  });
});
