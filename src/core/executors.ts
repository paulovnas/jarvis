import { z } from "zod";
import type { ModelChoice } from "./provider-references";

export const executorSchema = z.enum(["jarvis", "claude", "agy"]);
export type Executor = z.infer<typeof executorSchema>;
export type ExecutionChoice = { executor?: Executor; account: string; model: string; reasoning: string | null };
export type ExecutionSelection = { executor?: Executor; model: string; reasoning: string | null };
export const executorOf = (choice?: { executor?: Executor } | null): Executor => choice?.executor ?? "jarvis";
export const executionLabel = (choice: ExecutionChoice) => `${executorOf(choice) === "claude" ? "Claude Code" : executorOf(choice) === "agy" ? "Antigravity CLI" : choice.account} / ${choice.model}`;
export const sameExecutionTarget = (first: ExecutionChoice, second: ExecutionChoice) => executorOf(first) === executorOf(second) && first.account === second.account && normalizeExecutionSelection(first).model === normalizeExecutionSelection(second).model;

export function normalizeExecutionSelection(selection: ExecutionSelection): ExecutionSelection {
  if (executorOf(selection) !== "agy") return selection;
  const variant = /^(.+)-(low|medium|high|max)$/.exec(selection.model);
  return variant ? { ...selection, model: variant[1], reasoning: selection.reasoning ?? variant[2] } : selection;
}

export function selectModelChoice(current: ModelChoice | null | undefined, next: ExecutionChoice, slot: "primary" | "secondary"): ModelChoice {
  if (!current) return next;
  const { fallback, ...primary } = current;
  const opposite = slot === "primary" ? fallback : primary;
  if (opposite && sameExecutionTarget(next, opposite)) {
    // Without a secondary, there is no alternate primary to swap in.
    return fallback ? { ...fallback, fallback: primary } : current;
  }
  return slot === "primary" ? { ...current, ...next } : { ...current, fallback: next };
}

export function executionSelection(choice?: ExecutionChoice | null): ExecutionSelection | null {
  return choice ? normalizeExecutionSelection({ executor: executorOf(choice), model: executorOf(choice) !== "jarvis" ? choice.model : `${choice.account}/${choice.model}`, reasoning: choice.reasoning }) : null;
}

export function executionChoice(selection: ExecutionSelection): ExecutionChoice {
  const normalized = normalizeExecutionSelection(selection);
  if (executorOf(normalized) !== "jarvis") return { executor: executorOf(normalized), account: "", model: normalized.model, reasoning: normalized.reasoning };
  const split = normalized.model.indexOf("/");
  return { executor: "jarvis", account: split < 0 ? "" : normalized.model.slice(0, split), model: split < 0 ? normalized.model : normalized.model.slice(split + 1), reasoning: normalized.reasoning };
}

export const claudeProviderPreferencesSchema = z.object({
  enabled: z.boolean(),
  showUsage: z.boolean().optional(),
  disabledModels: z.array(z.string().min(1).max(256)).max(256),
});
export type ClaudeProviderPreferences = z.infer<typeof claudeProviderPreferencesSchema>;
export const DEFAULT_CLAUDE_PREFERENCES: ClaudeProviderPreferences = { enabled: true, showUsage: true, disabledModels: [] };

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
