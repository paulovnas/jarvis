import { z } from "zod";
import { pendingQuestionSchema } from "./questions";
import type { PendingQuestion } from "./questions";
import { attachmentSchema } from "./attachments";
import { pendingAuthoringSchema } from "./authoring";
import type { PendingAuthoring } from "./authoring";
import { executorSchema, type Executor } from "./executors";
import {
  IPC_PROTOCOL_VERSION,
  type ApprovalDecision as GeneratedApprovalDecision,
  type AgentStep as GeneratedAgentStep,
  type AgentTool as GeneratedAgentTool,
  type AgentTurn as GeneratedAgentTurn,
  type ChatSnapshot as GeneratedChatSnapshot,
  type CompactionEvent as GeneratedCompactionEvent,
  type ContextInfo as GeneratedContextInfo,
  type CoreActivity as GeneratedCoreActivity,
  type DirectTask as GeneratedDirectTask,
  type FileChange as GeneratedFileChange,
  type HistoryExcerpt as GeneratedHistoryExcerpt,
  type MessagePart as GeneratedMessagePart,
  type PendingApproval as GeneratedPendingApproval,
  type QueuedMessage as GeneratedQueuedMessage,
  type RetryStatus as GeneratedRetryStatus,
  type TurnOptions as GeneratedTurnOptions,
} from "@/generated/ipc";

export const messagePartSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("text"), text: z.string() }),
  z.object({ type: z.literal("skill"), id: z.string(), name: z.string() }),
  z.object({ type: z.literal("attachment"), attachment: attachmentSchema }),
]);
export type MessagePart = GeneratedMessagePart;
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
  executor: executorSchema.optional(),
  account: z.string(),
  model: z.string(),
  reasoning: z.string().nullable(),
  mode: z.enum(["plan", "build"]),
  workflow: z.enum(["standard", "designer", "planned", "complete", "publication", "custom"]).nullable().optional(),
  customWorkflowId: z.string().nullable().optional(),
  customAgentId: z.string().nullable().optional(),
  approvalMode: z.enum(["manual", "yolo"]),
  manualValidation: z.boolean().optional(),
});
export const agentToolSchema = z.object({
  id: z.string(), name: z.string(), args: z.record(z.string(), z.unknown()),
  status: z.enum(["pending", "running", "completed", "error"]),
  output: z.string(), durationMs: z.number().nonnegative(),
});
export const executionEffectsSchema = z.object({
  readsFilesystem: z.boolean(), writesFilesystem: z.boolean(), usesNetwork: z.boolean(),
  controlsProcesses: z.boolean(), destructive: z.boolean(), dynamic: z.boolean(), unknown: z.boolean(),
});
export const commandPlanSchema = z.object({
  invocations: z.array(z.object({ argv: z.array(z.string()) })),
  redirections: z.array(z.object({ target: z.string(), write: z.boolean() })),
  dynamic: z.boolean(),
});
export const sandboxReportSchema = z.object({
  backend: z.enum(["macosSeatbelt", "linuxBubblewrap", "windowsJobObject", "native"]),
  availability: z.enum(["full", "partial", "unavailable"]),
  filesystemIsolated: z.boolean(),
  network: z.enum(["isolated", "allowed", "native"]),
  processTreeIsolated: z.boolean(),
  reason: z.string().nullable(),
});
export const pendingApprovalSchema = z.object({
  tool: agentToolSchema,
  policy: z.object({
    code: z.string(), reason: z.string(), effects: executionEffectsSchema,
    command: commandPlanSchema.nullable(), readPaths: z.array(z.string()), writePaths: z.array(z.string()),
    workingDirectory: z.string(), repositoryRoot: z.string().nullable(), commandPrefixAvailable: z.boolean(),
    sandbox: sandboxReportSchema.nullable(),
  }).nullable(),
});
export type PendingApproval = GeneratedPendingApproval;
export type ApprovalDecision = GeneratedApprovalDecision;
export const retryStatusSchema = z.object({
  attempt: z.number().int().min(1).max(5), maxAttempts: z.literal(5),
  retryAt: z.number().nonnegative(), message: z.string(),
});
export type RetryStatus = GeneratedRetryStatus;
export const directTaskSchema = z.object({
  id: z.string(),
  title: z.string(),
  status: z.enum(["pending", "in_progress", "completed", "blocked"]),
});
export type DirectTask = GeneratedDirectTask;
export const usageSchema = z.object({
  inputTokens: z.number().nonnegative(),
  outputTokens: z.number().nonnegative(),
  cacheReadTokens: z.number().nonnegative().nullable().optional(),
  cacheWriteTokens: z.number().nonnegative().nullable().optional(),
});
export const agentStepSchema = z.object({
  coreActivities: z.array(z.object({
    component: z.enum(["context-mode", "ponytail", "beads", "open-design", "context7", "lsp"]),
    action: z.string(), status: z.enum(["applied", "reused", "unavailable", "pending", "issues"]),
    summary: z.string(), sources: z.array(z.string()), fingerprint: z.string().nullable().optional(),
    durationMs: z.number().nonnegative(),
  })).optional(),
  contextId: z.string().nullable().optional(),
  contextSearches: z.number().int().nonnegative().default(0),
  contextReductions: z.array(z.object({ callId: z.string(), originalBytes: z.number().nonnegative(), retainedBytes: z.number().nonnegative() })).optional(),
  readReuses: z.array(z.object({ callId: z.string(), originalBytes: z.number().nonnegative(), retainedBytes: z.number().nonnegative() })).optional(),
  loopSteers: z.number().int().nonnegative().optional(),
  loopAvoidedCalls: z.number().int().nonnegative().optional(),
  progressEvents: z.number().int().nonnegative().optional(),
  evidenceEvents: z.number().int().nonnegative().optional(),
  progressCheckpoints: z.number().int().nonnegative().optional(),
  progressPauses: z.number().int().nonnegative().optional(),
  durationMs: z.number().nonnegative(),
  text: z.string(), summary: z.string(), tools: z.array(agentToolSchema),
  retry: retryStatusSchema.nullable().optional(),
  usage: usageSchema.nullable(),
});
export const agentTurnSchema = z.object({
  id: z.string(), createdAt: z.number().nonnegative(), durationMs: z.number().nonnegative(),
  activeSince: z.number().nonnegative().nullable().optional(),
  user: z.string(), options: turnOptionsSchema,
  parts: z.array(messagePartSchema).default([]),
  contextWindow: z.number().int().positive().nullable().default(null),
  status: z.enum(["running", "completed", "cancelled", "error", "interrupted"]),
  tasks: z.array(directTaskSchema).default([]),
  steps: z.array(agentStepSchema),
  error: z.object({ code: z.string(), message: z.string() }).nullable(),
});
export const queuedMessageSchema = z.object({
  id: z.string(), content: z.string(), options: turnOptionsSchema,
  parts: z.array(messagePartSchema).default([]), auxiliaryFor: z.string().nullable().optional(),
});
export const fileChangeSchema = z.object({
  path: z.string(), additions: z.number().int().nonnegative().nullable(),
  deletions: z.number().int().nonnegative().nullable(), base: z.enum(["conversation", "git", "unknown"]),
  revision: z.number().int().nonnegative().default(0),
});
export const fileDiffSchema = z.object({
  path: z.string(), base: z.enum(["conversation", "git", "unknown"]), truncated: z.boolean(),
  rows: z.array(z.object({ kind: z.enum(["added", "removed", "context", "gap"]), oldLine: z.number().int().positive().nullable(), newLine: z.number().int().positive().nullable(), text: z.string() })),
});
export const contextInfoSchema = z.object({
  tokens: z.number().nonnegative(), limit: z.number().positive().nullable(),
  estimated: z.boolean(), compacting: z.boolean(), compactions: z.number().int().nonnegative(),
});
export const compactionEventSchema = z.object({
  id: z.string(), createdAt: z.number().nonnegative(), turnId: z.string(),
  afterTurn: z.boolean(), automatic: z.boolean(),
  tokensBefore: z.number().nonnegative(), tokensAfter: z.number().nonnegative(),
});
export type CompactionEvent = GeneratedCompactionEvent;
export const historyWindowSchema = z.object({ start: z.number().int().nonnegative(), total: z.number().int().nonnegative() });
export const historyExcerptSchema = z.object({ id: z.string(), index: z.number().int().nonnegative(), createdAt: z.number().nonnegative(), user: z.string(), assistant: z.string() });
export type HistoryExcerpt = GeneratedHistoryExcerpt;
export const historyPageSchema = z.object({ conversationId: z.string(), turns: z.array(agentTurnSchema), compactions: z.array(compactionEventSchema), history: historyWindowSchema, navigation: z.array(historyExcerptSchema) });
export interface HistoryPage {
  conversationId: string;
  turns: AgentTurn[];
  compactions: CompactionEvent[];
  history: { start: number; total: number };
  navigation: HistoryExcerpt[];
}
const snapshotSchema = z.object({
  protocolVersion: z.number().int().nonnegative().default(0),
  conversationId: z.string(), revision: z.number().int().nonnegative(),
  turns: z.array(agentTurnSchema), activeTurnId: z.string().nullable(), pendingApproval: pendingApprovalSchema.nullable(),
  queuedMessages: z.array(queuedMessageSchema).default([]),
  context: contextInfoSchema.default({ tokens: 0, limit: null, estimated: true, compacting: false, compactions: 0 }),
  fileChanges: z.array(fileChangeSchema).default([]),
  compacting: z.boolean().default(false),
  compactions: z.array(compactionEventSchema).default([]),
  pendingQuestion: pendingQuestionSchema.nullable().default(null),
  pendingAuthoring: pendingAuthoringSchema.nullable().optional(),
  history: historyWindowSchema.optional(), navigation: z.array(historyExcerptSchema).optional(),
  latestOptions: turnOptionsSchema.optional(),
});
export type QueuedMessage = Omit<GeneratedQueuedMessage, "parts"> & { parts?: MessagePart[] };
export type FileChange = Omit<GeneratedFileChange, "revision"> & { revision?: number };
export type FileDiff = z.infer<typeof fileDiffSchema>;
export type ContextInfo = GeneratedContextInfo;
export type CoreActivity = GeneratedCoreActivity;
export type TurnOptions = Omit<GeneratedTurnOptions, "executor"> & { executor?: Executor };
export type AgentStep = Omit<GeneratedAgentStep, "contextSearches"> & { contextSearches?: number };
export type AgentTurn = Omit<GeneratedAgentTurn, "parts" | "contextWindow" | "steps"> & {
  parts?: MessagePart[];
  contextWindow?: number | null;
  steps: AgentStep[];
};
export type AgentTool = GeneratedAgentTool;
type LegacySnapshotDefaults = "protocolVersion" | "compacting" | "context" | "compactions" | "history";
export type ChatSnapshot = Omit<GeneratedChatSnapshot, "turns" | "pendingQuestion" | "pendingAuthoring" | "queuedMessages" | "fileChanges" | LegacySnapshotDefaults> &
Partial<Pick<GeneratedChatSnapshot, LegacySnapshotDefaults>> & {
  turns: AgentTurn[];
  queuedMessages?: QueuedMessage[];
  fileChanges?: FileChange[];
  pendingQuestion?: PendingQuestion | null;
  pendingAuthoring?: PendingAuthoring | null;
  latestOptions?: TurnOptions;
};

export const agentActivitySchema = snapshotSchema.pick({ conversationId: true, revision: true, activeTurnId: true, compacting: true });

export function readChat(value: unknown, conversationId: string): ChatSnapshot {
  const result = snapshotSchema.safeParse(value);
  if (!result.success || result.data.conversationId !== conversationId || result.data.protocolVersion > IPC_PROTOCOL_VERSION) {
    throw new Error("O histórico recebido não corresponde à conversa selecionada.");
  }
  const snapshot = result.data;
  return {
    ...snapshot,
    history: snapshot.history ?? { start: 0, total: snapshot.turns.length },
  } as ChatSnapshot;
}
