import { z } from "zod";

export const turnOptionsSchema = z.object({
  account: z.string(),
  model: z.string(),
  reasoning: z.string().nullable(),
  mode: z.enum(["plan", "build"]),
  approvalMode: z.enum(["manual", "yolo"]),
});
const toolSchema = z.object({
  id: z.string(), name: z.string(), args: z.record(z.string(), z.unknown()),
  status: z.enum(["pending", "running", "completed", "error"]),
  output: z.string(), durationMs: z.number().nonnegative(),
});
const turnSchema = z.object({
  id: z.string(), createdAt: z.number().nonnegative(), durationMs: z.number().nonnegative(),
  user: z.string(), options: turnOptionsSchema,
  status: z.enum(["running", "completed", "cancelled", "error", "interrupted"]),
  steps: z.array(z.object({
    durationMs: z.number().nonnegative(),
    text: z.string(), summary: z.string(), tools: z.array(toolSchema),
    usage: z.object({ inputTokens: z.number().nonnegative(), outputTokens: z.number().nonnegative() }).nullable(),
  })),
  error: z.object({ code: z.string(), message: z.string() }).nullable(),
});
const snapshotSchema = z.object({
  conversationId: z.string(), revision: z.number().int().nonnegative(),
  turns: z.array(turnSchema), activeTurnId: z.string().nullable(), pendingApproval: toolSchema.nullable(),
});
export type TurnOptions = z.infer<typeof turnOptionsSchema>;
export type AgentTurn = z.infer<typeof turnSchema>;
export type AgentTool = z.infer<typeof toolSchema>;
export type ChatSnapshot = z.infer<typeof snapshotSchema>;

export const agentActivitySchema = snapshotSchema.pick({ conversationId: true, revision: true, activeTurnId: true });

export function readChat(value: unknown, conversationId: string): ChatSnapshot {
  const result = snapshotSchema.safeParse(value);
  if (!result.success || result.data.conversationId !== conversationId) {
    throw new Error("O histórico recebido não corresponde à conversa selecionada.");
  }
  return result.data;
}
