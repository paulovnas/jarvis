import { z } from "zod";
import { modelChoiceSchema } from "./workflow-catalog";
import type { ProviderAccount, ProviderModel } from "./provider-accounts";

export type ModelChoice = z.infer<typeof modelChoiceSchema>;
export const providerReferenceSchema = z.object({
  id: z.string(), itemKey: z.string(),
  kind: z.enum(["web_search", "vision", "image_generation", "builtin_agent", "custom_agent", "conversation"]),
  label: z.string(), details: z.array(z.string()), choice: modelChoiceSchema,
});
export const modelBindingSchema = z.object({ itemKey: z.string(), source: modelChoiceSchema, target: modelChoiceSchema });
export const providerRemovalPlanSchema = z.object({ alias: z.string(), revision: z.string(), items: z.array(providerReferenceSchema) });
export const providerRemovalResultSchema = z.object({ replaced: z.number().int().nonnegative(), unresolved: z.array(providerReferenceSchema) });
export const providerReferencesSchema = z.object({ references: z.array(providerReferenceSchema), bindings: z.array(modelBindingSchema) });
export type ProviderReference = z.infer<typeof providerReferenceSchema>;
export type ProviderRemovalPlan = z.infer<typeof providerRemovalPlanSchema>;
export type ProviderRemovalResult = z.infer<typeof providerRemovalResultSchema>;
export type ModelBinding = z.infer<typeof modelBindingSchema>;
export type ReferenceKind = ProviderReference["kind"];

const imageModel: ProviderModel = { id: "gemini-3.1-flash-image", name: "Gemini 3.1 Flash Image", reasoningLevels: [], defaultReasoningLevel: null };
export function compatibleModels(account: ProviderAccount, kind: ReferenceKind): ProviderModel[] {
  if (!account.enabled) return [];
  if (kind === "image_generation") return account.providerKind === "antigravity" ? [imageModel] : [];
  if (!account.modelsAvailable) return [];
  return account.models.filter(model => kind === "web_search" ? account.providerKind === "openai-codex" || (account.providerKind === "antigravity" && model.id.startsWith("gemini-")) : kind === "vision" ? account.providerKind === "custom" ? account.custom?.models.some(item => item.id === model.id && item.supportsImages) : /^(gpt-|gemini-|claude|o3|o4)/.test(model.id) : true);
}

export function modelProblem(choice: ModelChoice, accounts: ProviderAccount[], kind: ReferenceKind = "custom_agent"): string | null {
  const account = accounts.find(account => account.alias === choice.account);
  if (!account) return `O provedor ${choice.account} não existe mais. Escolha outro provedor e modelo.`;
  if (!account.enabled) return `O provedor ${choice.account} está desativado. Ative-o ou escolha outro.`;
  if (kind !== "image_generation" && !account.modelsAvailable) return `Os modelos de ${choice.account} estão indisponíveis. Revise a conexão do provedor.`;
  const model = compatibleModels(account, kind).find(model => model.id === choice.model);
  if (!model) return `O modelo ${choice.model || "configurado"} não está disponível para este item. Escolha outro modelo.`;
  if (choice.reasoning && !model.reasoningLevels.includes(choice.reasoning)) return "O nível de raciocínio configurado não está disponível neste modelo. Revise a seleção.";
  return null;
}

export function resolveChatModel(bindings: ModelBinding[], conversationId: string | undefined, choice: ModelChoice): ModelChoice {
  return bindings.find(binding => binding.itemKey === `chat:${conversationId}` && binding.source.account === choice.account && binding.source.model === choice.model && binding.source.reasoning === choice.reasoning)?.target ?? choice;
}

export function defaultChoice(account: ProviderAccount, model: ProviderModel): ModelChoice {
  return { account: account.alias, model: model.id, reasoning: model.defaultReasoningLevel ?? model.reasoningLevels[0] ?? null };
}
