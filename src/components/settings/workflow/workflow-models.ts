import type { ProviderAccount } from "@/core/provider-accounts";

export const accountGroups = (accounts: ProviderAccount[]) => accounts.filter(a => a.enabled && a.modelsAvailable).map(a => ({ provider: a.alias, models: a.models.map(m => ({ value: `${a.alias}/${m.id}`, label: m.name, reasoningLevels: m.reasoningLevels, defaultReasoningLevel: m.defaultReasoningLevel })) }));
