import { z } from "zod";
import { pendingQuestionSchema } from "./questions";
import { attachmentSchema } from "./attachments";
import { pendingAuthoringSchema } from "./authoring";

export const messagePartSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("text"), text: z.string() }),
  z.object({ type: z.literal("skill"), id: z.string(), name: z.string() }),
  z.object({ type: z.literal("attachment"), attachment: attachmentSchema }),
]);
export type MessagePart = z.infer<typeof messagePartSchema>;
export interface ChatDraft { content: string; parts?: MessagePart[] }
export function draftText(parts: MessagePart[]): string {
  return parts.map(part => part.type === "text" ? part.text : part.type === "skill" ? `/${part.name}` : "").join("");
}
export function mergeDrafts(current: ChatDraft, restored: ChatDraft): ChatDraft {
  const parts = [...(current.parts ?? (current.content ? [{ type: "text" as const, text: current.content }] : [])),
    ...(current.content ? [{ type: "text" as const, text: "\n\n" }] : []),
    ...(restored.parts ?? [{ type: "text" as const, text: restored.content }])];
  const seen = new Set<string>();
  const unique = parts.filter(part => { if (part.type !== "attachment") return true; if (seen.has(part.attachment.id)) return false; seen.add(part.attachment.id); return true; });
  return { content: draftText(unique), parts: unique };
}

export const turnOptionsSchema = z.object({
  account: z.string(),
  model: z.string(),
  reasoning: z.string().nullable(),
  mode: z.enum(["plan", "build"]),
  workflow: z.enum(["standard", "designer", "planned", "complete", "custom"]).optional(),
  customWorkflowId: z.string().optional(),
  customAgentId: z.string().optional(),
  approvalMode: z.enum(["manual", "yolo"]),
  manualValidation: z.boolean().optional(),
});
export const agentToolSchema = z.object({
  id: z.string(), name: z.string(), args: z.record(z.string(), z.unknown()),
  status: z.enum(["pending", "running", "completed", "error"]),
  output: z.string(), durationMs: z.number().nonnegative(),
});
const retryStatusSchema = z.object({
  attempt: z.number().int().min(1).max(5), maxAttempts: z.literal(5),
  retryAt: z.number().nonnegative(), message: z.string(),
});
export type RetryStatus = z.infer<typeof retryStatusSchema>;
export const directTaskSchema = z.object({
  id: z.string(),
  title: z.string(),
  status: z.enum(["pending", "in_progress", "completed", "blocked"]),
});
export type DirectTask = z.infer<typeof directTaskSchema>;
const turnSchema = z.object({
  id: z.string(), createdAt: z.number().nonnegative(), durationMs: z.number().nonnegative(),
  user: z.string(), options: turnOptionsSchema,
  parts: z.array(messagePartSchema).optional(),
  contextWindow: z.number().int().positive().nullable().optional(),
  status: z.enum(["running", "completed", "cancelled", "error", "interrupted"]),
  tasks: z.array(directTaskSchema).default([]),
  steps: z.array(z.object({
    durationMs: z.number().nonnegative(),
    text: z.string(), summary: z.string(), tools: z.array(agentToolSchema),
    retry: retryStatusSchema.nullable().optional(),
    usage: z.object({ inputTokens: z.number().nonnegative(), outputTokens: z.number().nonnegative() }).nullable(),
  })),
  error: z.object({ code: z.string(), message: z.string() }).nullable(),
});
export const queuedMessageSchema = z.object({ id: z.string(), content: z.string(), options: turnOptionsSchema, parts: z.array(messagePartSchema).optional() });
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
const compactionEventSchema = z.object({
  id: z.string(), createdAt: z.number().nonnegative(), turnId: z.string(),
  afterTurn: z.boolean(), automatic: z.boolean(),
  tokensBefore: z.number().nonnegative(), tokensAfter: z.number().nonnegative(),
});
export type CompactionEvent = z.infer<typeof compactionEventSchema>;
export const historyWindowSchema = z.object({ start: z.number().int().nonnegative(), total: z.number().int().nonnegative() });
export const historyExcerptSchema = z.object({ id: z.string(), index: z.number().int().nonnegative(), createdAt: z.number().nonnegative(), user: z.string(), assistant: z.string() });
export type HistoryExcerpt = z.infer<typeof historyExcerptSchema>;
export const historyPageSchema = z.object({ conversationId: z.string(), turns: z.array(turnSchema), compactions: z.array(compactionEventSchema), history: historyWindowSchema, navigation: z.array(historyExcerptSchema) });
export type HistoryPage = z.infer<typeof historyPageSchema>;
const snapshotSchema = z.object({
  conversationId: z.string(), revision: z.number().int().nonnegative(),
  turns: z.array(turnSchema), activeTurnId: z.string().nullable(), pendingApproval: agentToolSchema.nullable(),
  queuedMessages: z.array(queuedMessageSchema).optional(), context: contextInfoSchema.optional(), fileChanges: z.array(fileChangeSchema).optional(),
  compacting: z.boolean().optional(),
  compactions: z.array(compactionEventSchema).optional(),
  pendingQuestion: pendingQuestionSchema.nullable().optional(),
  pendingAuthoring: pendingAuthoringSchema.nullable().optional(),
  history: historyWindowSchema.optional(), navigation: z.array(historyExcerptSchema).optional(),
  latestOptions: turnOptionsSchema.optional(),
});
export type QueuedMessage = z.infer<typeof queuedMessageSchema>;
export type FileChange = z.infer<typeof fileChangeSchema>;
export type FileDiff = z.infer<typeof fileDiffSchema>;
export type ContextInfo = z.infer<typeof contextInfoSchema>;
export type TurnOptions = z.infer<typeof turnOptionsSchema>;
export type AgentTurn = z.infer<typeof turnSchema>;
export type AgentTool = z.infer<typeof agentToolSchema>;
export type ChatSnapshot = z.infer<typeof snapshotSchema>;

export const agentActivitySchema = snapshotSchema.pick({ conversationId: true, revision: true, activeTurnId: true, compacting: true });

export function readChat(value: unknown, conversationId: string): ChatSnapshot {
  const result = snapshotSchema.safeParse(value);
  if (!result.success || result.data.conversationId !== conversationId) {
    throw new Error("O histórico recebido não corresponde à conversa selecionada.");
  }
  return result.data;
}
