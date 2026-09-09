import { z } from "zod";

export const questionPreviewSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("wireframe"), elements: z.array(z.object({ label: z.string().min(1).max(60), x: z.number().min(0), y: z.number().min(0), width: z.number().min(5), height: z.number().min(5) }).refine(e => e.x + e.width <= 100 && e.y + e.height <= 100)).min(1).max(16) }),
  z.object({ type: z.literal("palette"), colors: z.array(z.string().regex(/^#[a-fA-F0-9]{6}$/)).min(2).max(8), sample: z.string().min(1).max(160) }),
  z.object({ type: z.literal("ascii"), text: z.string().min(1).max(6000).refine(text => text.split("\n").length <= 40 && text.split("\n").every(line => line.length <= 120)) }),
]);
export type QuestionPreview = z.infer<typeof questionPreviewSchema>;

export const questionSchema = z.object({
  id: z.string().min(1).max(64),
  question: z.string().min(1).max(1000),
  options: z.array(z.object({ label: z.string().min(1).max(200), description: z.string().max(500).nullish(), preview: questionPreviewSchema.nullish(), recommended: z.boolean().optional() })).max(6)
    .refine(options => new Set(options.map(option => option.label.trim())).size === options.length)
    .refine(options => options.filter(option => option.recommended).length <= 1).default([]),
});
export const questionRequestSchema = z.object({ questions: z.array(questionSchema).min(1).max(3).refine(questions => new Set(questions.map(question => question.id)).size === questions.length) });
export const pendingQuestionSchema = questionRequestSchema.extend({ turnId: z.string(), toolId: z.string(), deadlineAt: z.number().int().positive().optional() });
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
