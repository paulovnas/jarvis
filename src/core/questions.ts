import { z } from "zod";

export const questionSchema = z.object({
  id: z.string().min(1).max(64),
  question: z.string().min(1).max(1000),
  options: z.array(z.object({ label: z.string().min(1).max(200), description: z.string().max(500).nullish() })).max(6)
    .refine(options => new Set(options.map(option => option.label.trim())).size === options.length).default([]),
});
export const questionRequestSchema = z.object({ questions: z.array(questionSchema).min(1).max(3).refine(questions => new Set(questions.map(question => question.id)).size === questions.length) });
export const pendingQuestionSchema = questionRequestSchema.extend({ turnId: z.string(), toolId: z.string() });
export const questionResponseSchema = z.object({
  cancelled: z.boolean(),
  answers: z.array(z.object({ id: z.string(), value: z.string().min(1).max(4000), selectedLabel: z.string().optional() })),
});
export type PendingQuestion = z.infer<typeof pendingQuestionSchema>;
export type QuestionResponse = z.infer<typeof questionResponseSchema>;
export type QuestionAnswer = QuestionResponse["answers"][number];
export interface QuestionDraft { index: number; answers: Record<string, QuestionAnswer>; custom: Record<string, string> }
export function questionKey(conversationId: string, request: PendingQuestion): string {
  return JSON.stringify([conversationId, request.turnId, request.toolId]);
}

export function readQuestionResponse(output?: string): QuestionResponse | null {
  try {
    const result = questionResponseSchema.safeParse(JSON.parse(output ?? ""));
    return result.success ? result.data : null;
  } catch { return null; }
}
