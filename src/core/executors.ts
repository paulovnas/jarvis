import { z } from "zod";
import type { ModelChoice } from "./provider-references";

// Retired executors remain readable in saved history without becoming runnable.
export const executorSchema = z.string().transform(value => value === "jarvis" ? "jarvis" as const : value === "claude" ? "claude" as const : "unavailable" as const);
export type Executor = z.infer<typeof executorSchema>;
export const serviceTierSchema = z.literal("priority").nullable().transform(value => value ?? undefined).optional();
export type ServiceTier = z.infer<typeof serviceTierSchema>;
export type ExecutionChoice = { executor?: Executor; account: string; model: string; reasoning: string | null; serviceTier?: ServiceTier };
export type ExecutionSelection = { executor?: Executor; model: string; reasoning: string | null; serviceTier?: ServiceTier };
export const executorOf = (choice?: { executor?: Executor } | null): Executor => choice?.executor ?? "jarvis";
export const executionLabel = (choice: ExecutionChoice) => `${executorOf(choice) === "claude" ? "Claude Code" : executorOf(choice) === "unavailable" ? "Executor removido" : choice.account} / ${choice.model}${choice.serviceTier === "priority" ? " · Fast" : ""}`;
export const sameExecutionTarget = (first: ExecutionChoice, second: ExecutionChoice) => executorOf(first) === executorOf(second) && first.account === second.account && first.model === second.model;
export const FAST_USAGE_NOTICE = "Maior consumo dos limites/créditos";
export function supportsFastMode(providerKind: string | undefined, model: { supportsFast?: boolean } | null | undefined, executor?: Executor): boolean {
  return executorOf({ executor }) === "jarvis" && providerKind === "openai-codex" && model?.supportsFast === true;
}

export function fastModeUnavailable(providerKind: string | undefined, model: { supportsFast?: boolean } | null | undefined, executor?: Executor, modelsStale = false): boolean {
  return !supportsFastMode(providerKind, model, executor) && !(modelsStale && providerKind === "openai-codex" && executorOf({ executor }) === "jarvis");
}

export function selectModelChoice(current: ModelChoice | null | undefined, next: ExecutionChoice, slot: "primary" | "secondary"): ModelChoice {
  if (!current) return next;
  const { fallback, ...primary } = current;
  const opposite = slot === "primary" ? fallback : primary;
  if (opposite && sameExecutionTarget(next, opposite)) {
    // Without a secondary, there is no alternate primary to swap in.
    return fallback ? { ...fallback, fallback: primary } : current;
  }
  if (slot === "secondary") return { ...current, fallback: next };
  const updated = { ...current, ...next };
  if (next.serviceTier !== "priority") delete updated.serviceTier;
  return updated;
}

export function executionSelection(choice?: ExecutionChoice | null): ExecutionSelection | null {
  return choice ? { executor: executorOf(choice), model: executorOf(choice) !== "jarvis" ? choice.model : `${choice.account}/${choice.model}`, reasoning: choice.reasoning, ...(choice.serviceTier === "priority" ? { serviceTier: choice.serviceTier } : {}) } : null;
}

export function executionChoice(selection: ExecutionSelection): ExecutionChoice {
  const tier = selection.serviceTier === "priority" ? { serviceTier: selection.serviceTier } : {};
  if (executorOf(selection) !== "jarvis") return { executor: executorOf(selection), account: "", model: selection.model, reasoning: selection.reasoning, ...tier };
  const split = selection.model.indexOf("/");
  return { executor: "jarvis", account: split < 0 ? "" : selection.model.slice(0, split), model: split < 0 ? selection.model : selection.model.slice(split + 1), reasoning: selection.reasoning, ...tier };
}

export const claudeProviderPreferencesSchema = z.object({
  enabled: z.boolean(),
  showUsage: z.boolean().optional(),
  showFiveHourUsage: z.boolean().optional(),
  showWeeklyUsage: z.boolean().optional(),
  disabledModels: z.array(z.string().min(1).max(256)).max(256),
});
export type ClaudeProviderPreferences = z.infer<typeof claudeProviderPreferencesSchema>;
export const DEFAULT_CLAUDE_PREFERENCES: ClaudeProviderPreferences = { enabled: true, showUsage: true, showFiveHourUsage: true, showWeeklyUsage: true, disabledModels: [] };

export const claudeRuntimeSchema = z.object({
  preferences: claudeProviderPreferencesSchema.optional(),
  installed: z.boolean(), authenticated: z.boolean(), version: z.string().nullable(),
  models: z.array(z.object({ id: z.string(), name: z.string(), description: z.string(), reasoningLevels: z.array(z.string()), defaultReasoning: z.string().nullable() })),
  error: z.string().nullable(), authMethod: z.string().nullish(), email: z.string().nullish(), subscriptionType: z.string().nullish(),
});
export type ClaudeRuntime = z.infer<typeof claudeRuntimeSchema>;
export function claudeModels(runtime: ClaudeRuntime | null) {
  const preferences = runtime?.preferences ?? DEFAULT_CLAUDE_PREFERENCES;
  return preferences.enabled ? runtime?.models.filter(model => !preferences.disabledModels.includes(model.id)).map(model => ({ value: model.id, label: model.name, reasoningLevels: model.reasoningLevels, defaultReasoningLevel: model.defaultReasoning })) ?? [] : [];
}
