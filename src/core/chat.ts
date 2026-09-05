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
  contextWindow: z.number().int().positive().nullable().optional(),
  status: z.enum(["running", "completed", "cancelled", "error", "interrupted"]),
  steps: z.array(z.object({
    durationMs: z.number().nonnegative(),
    text: z.string(), summary: z.string(), tools: z.array(toolSchema),
    usage: z.object({ inputTokens: z.number().nonnegative(), outputTokens: z.number().nonnegative() }).nullable(),
  })),
  error: z.object({ code: z.string(), message: z.string() }).nullable(),
});
export const queuedMessageSchema = z.object({ id: z.string(), content: z.string(), options: turnOptionsSchema });
export const fileChangeSchema = z.object({
  path: z.string(), additions: z.number().int().nonnegative().nullable(),
  deletions: z.number().int().nonnegative().nullable(), base: z.enum(["conversation", "git", "unknown"]),
  revision: z.number().int().nonnegative().optional(),
});
export const fileDiffSchema = z.object({
  path: z.string(), base: z.enum(["conversation", "git", "unknown"]), truncated: z.boolean(),
  rows: z.array(z.object({ kind: z.enum(["added", "removed", "context", "gap"]), oldLine: z.number().int().positive().nullable(), newLine: z.number().int().positive().nullable(), text: z.string() })),
});
const contextInfoSchema = z.object({
  tokens: z.number().nonnegative(), limit: z.number().positive().nullable(),
  estimated: z.boolean(), compacting: z.boolean(), compactions: z.number().int().nonnegative(),
});
const snapshotSchema = z.object({
  conversationId: z.string(), revision: z.number().int().nonnegative(),
  turns: z.array(turnSchema), activeTurnId: z.string().nullable(), pendingApproval: toolSchema.nullable(),
  queuedMessages: z.array(queuedMessageSchema).optional(), context: contextInfoSchema.optional(), fileChanges: z.array(fileChangeSchema).optional(),
  compacting: z.boolean().optional(),
});
export type QueuedMessage = z.infer<typeof queuedMessageSchema>;
export type FileChange = z.infer<typeof fileChangeSchema>;
export type FileDiff = z.infer<typeof fileDiffSchema>;
export type ContextInfo = z.infer<typeof contextInfoSchema>;
export type TurnOptions = z.infer<typeof turnOptionsSchema>;
export type AgentTurn = z.infer<typeof turnSchema>;
export type AgentTool = z.infer<typeof toolSchema>;
export type ChatSnapshot = z.infer<typeof snapshotSchema>;

export const agentActivitySchema = snapshotSchema.pick({ conversationId: true, revision: true, activeTurnId: true, compacting: true });

export function readChat(value: unknown, conversationId: string): ChatSnapshot {
  const result = snapshotSchema.safeParse(value);
  if (!result.success || result.data.conversationId !== conversationId) {
    throw new Error("O histórico recebido não corresponde à conversa selecionada.");
  }
  return result.data;
}
