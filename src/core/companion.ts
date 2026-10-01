import { z } from "zod";
import { pendingQuestionSchema } from "./questions";
import { readChat, turnOptionsSchema } from "./chat";

export const companionStatusSchema = z.enum(["running", "waiting", "reconnecting", "completed", "failed", "idle"]);
export const companionItemSchema = z.object({
  conversationId: z.string(), agentId: z.string().nullable(), projectId: z.string(), projectName: z.string(),
  title: z.string(), role: z.string(), status: companionStatusSchema,
  activity: z.string(), result: z.string().nullish(), durationMs: z.number().nonnegative(), activeSince: z.number().nullable(), updatedAt: z.number(),
  pendingQuestion: pendingQuestionSchema.nullish(), requiresConversation: z.boolean(),
  attentionId: z.string(), acknowledged: z.boolean(),
});
export const companionSnapshotSchema = z.object({ items: z.array(companionItemSchema), truncated: z.boolean() });
export const companionGeometrySchema = z.object({
  expanded: z.boolean(), bubble: z.boolean(), robotSide: z.enum(["left", "right"]), robotVertical: z.enum(["top", "bottom"]),
  width: z.number().positive(), height: z.number().positive(),
});
export const companionProjectProposalSchema = z.object({
  id: z.string(), projectId: z.string(), projectName: z.string(), workspaceName: z.string(),
  conversationId: z.string().nullable(), reason: z.string(), message: z.string(),
});
export const companionChatSchema = z.object({
  conversationId: z.string(), projectId: z.string().nullable(), projectName: z.string().nullable(),
  global: z.boolean(), chat: z.unknown(), options: turnOptionsSchema.nullish(), proposal: companionProjectProposalSchema.nullish(),
}).transform((value, context) => {
  try { return { ...value, chat: readChat(value.chat, value.conversationId) }; }
  catch { context.addIssue({ code: "custom", message: "O histórico recebido não corresponde à conversa selecionada.", path: ["chat"] }); return z.NEVER; }
});
export const companionConversationsSchema = z.array(z.object({
  id: z.string(), projectId: z.string(), projectName: z.string(), workspaceName: z.string(),
  title: z.string(), lastActivityAt: z.number(),
}));
export const companionModelsSchema = z.array(z.object({
  provider: z.string(), providerKind: z.string().optional(), executor: z.literal("claude").optional(),
  models: z.array(z.object({ value: z.string(), label: z.string(), reasoningLevels: z.array(z.string()), defaultReasoningLevel: z.string().nullable() })),
  emptyMessage: z.string().optional(),
}));
export type CompanionItem = z.infer<typeof companionItemSchema>;
export type CompanionStatus = z.infer<typeof companionStatusSchema>;
export type CompanionSnapshot = z.infer<typeof companionSnapshotSchema>;
export type CompanionGeometry = z.infer<typeof companionGeometrySchema>;
export type CompanionChat = z.infer<typeof companionChatSchema>;
export type CompanionConversation = z.infer<typeof companionConversationsSchema>[number];
export const companionItemKey = (item: CompanionItem) => `${item.conversationId}/${item.agentId ?? "root"}`;
export const companionStatusLabels: Record<CompanionStatus, string> = {
  running: "Trabalhando", waiting: "Precisa de você", reconnecting: "Reconectando", completed: "Concluído", failed: "Falhou", idle: "Em repouso",
};
