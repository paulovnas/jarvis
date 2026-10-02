import { z } from "zod";
import type { AgentTurn } from "./chat";
import type { PendingQuestion, QuestionAnswer } from "./questions";

export const voiceConfigSchema = z.object({
  enabled: z.boolean(), microphone: z.string().nullable(), speaker: z.string().nullable(),
  model: z.enum(["small", "tiny"]), voice: z.enum(["pf_dora", "pm_alex", "pm_santa"]),
  speed: z.number().min(0.75).max(1.5), silenceMs: z.number().int().min(400).max(1800),
});
export const voiceSessionSchema = z.object({
  id: z.string().nullable(), target: z.string().nullable(), owner: z.string().nullable(),
  mode: z.enum(["dictation", "call", "test"]).nullable(),
  phase: z.enum(["idle", "preparing", "listening", "transcribing", "thinking", "synthesizing", "speaking", "paused", "closing", "error"]),
  muted: z.boolean(), level: z.number().min(0).max(1), transcript: z.string(), error: z.string().nullable(), revision: z.number().int().nonnegative(),
  startedAt: z.number().nonnegative().nullable(), speaker: z.enum(["user", "jarvis"]).nullable(),
});
export const voiceDownloadSchema = z.object({ model: z.string(), received: z.number().nonnegative(), total: z.number().positive() });
export const voiceSettingsSchema = z.object({
  config: voiceConfigSchema,
  microphones: z.array(z.object({ id: z.string(), name: z.string() })),
  speakers: z.array(z.object({ id: z.string(), name: z.string() })),
  models: z.array(z.object({ id: z.string(), name: z.string(), bytes: z.number().positive(), installed: z.boolean() })),
  speechReady: z.boolean(), speechError: z.string().nullable(), download: voiceDownloadSchema.nullable(), session: voiceSessionSchema,
});
export const voiceTranscriptSchema = z.object({ sessionId: z.string(), target: z.string(), sequence: z.number().int().positive(), text: z.string().min(1), mode: z.enum(["dictation", "call"]) });
export type VoiceConfig = z.infer<typeof voiceConfigSchema>;
export type VoiceSession = z.infer<typeof voiceSessionSchema>;
export type VoiceSettings = z.infer<typeof voiceSettingsSchema>;
export const voiceActive = (session: VoiceSession | null | undefined) => !!session?.id && session.phase !== "idle" && session.phase !== "error";
export const voicePhaseLabels: Record<VoiceSession["phase"], string> = {
  idle: "Ligação encerrada", preparing: "Preparando voz local…", listening: "Estou ouvindo", transcribing: "Entendendo sua fala…",
  thinking: "Pensando…", synthesizing: "Preparando a resposta…", speaking: "Falando", paused: "Microfone pausado", closing: "Encerrando voz…", error: "Voz interrompida",
};

export function voiceReply(turn: AgentTurn): string {
  if (turn.status === "error") return turn.error?.message ?? "Não consegui concluir esta solicitação. Veja os detalhes na conversa.";
  if (turn.status === "cancelled" || turn.status === "interrupted") return "A execução foi interrompida. Podemos continuar quando você quiser.";
  return [...turn.steps].reverse().find(step => step.text.trim())?.text ?? "Concluí a solicitação. Os detalhes estão na conversa.";
}
export function voiceQuestionPrompt(question: PendingQuestion["questions"][number]): string {
  return `${question.question}${question.options.length ? ` Opções: ${question.options.map((option, index) => `${index + 1}, ${option.label}`).join(". ")}. Você pode responder com o número ou com suas palavras.` : " Pode responder com suas palavras."}`;
}
export function voiceQuestionAnswer(question: PendingQuestion["questions"][number], text: string): QuestionAnswer {
  const normalize = (value: string) => value.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLocaleLowerCase("pt-BR").replace(/[.!?]+$/u, "").trim();
  const value = normalize(text);
  const numbered = value.match(/^(?:opcao\s+)?([1-6]|um|uma|dois|duas|tres|quatro|cinco|seis|primeira|segunda|terceira|quarta|quinta|sexta)$/u);
  const numbers: Record<string, number> = { um: 1, uma: 1, dois: 2, duas: 2, tres: 3, quatro: 4, cinco: 5, seis: 6, primeira: 1, segunda: 2, terceira: 3, quarta: 4, quinta: 5, sexta: 6 };
  const index = numbered ? (numbers[numbered[1]] ?? Number(numbered[1])) - 1 : -1;
  const option = question.options.find(option => normalize(option.label) === value) ?? question.options[index];
  return { id: question.id, value: option?.label ?? text.trim(), ...(option ? { selectedLabel: option.label } : {}) };
}
