import { z } from "zod";
import { claudeProviderPreferencesSchema, claudeRuntimeSchema } from "./executors";

export const agyProviderPreferencesSchema = claudeProviderPreferencesSchema;
export type AgyProviderPreferences = z.infer<typeof agyProviderPreferencesSchema>;
export const DEFAULT_AGY_PREFERENCES: AgyProviderPreferences = { enabled: false, showUsage: true, disabledModels: [] };
export const agyRuntimeSchema = claudeRuntimeSchema.extend({ preferences: agyProviderPreferencesSchema.optional() });
export type AgyRuntime = z.infer<typeof agyRuntimeSchema>;

export function agyModels(runtime: AgyRuntime | null) {
  const preferences = runtime?.preferences ?? DEFAULT_AGY_PREFERENCES;
  return preferences.enabled ? runtime?.models.filter(model => !preferences.disabledModels.includes(model.id)).map(model => ({ value: model.id, label: model.name, reasoningLevels: model.reasoningLevels, defaultReasoningLevel: model.defaultReasoning })) ?? [] : [];
}
