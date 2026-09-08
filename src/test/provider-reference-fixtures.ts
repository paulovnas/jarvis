import type { ProviderAccount } from "@/core/provider-accounts";
import type { ProviderReference } from "@/core/provider-references";

export const referenceAccount = (alias = "novo"): ProviderAccount => ({ alias, enabled: true, providerKind: "openai-codex", modelsAvailable: true, createdAt: 1, email: null, accountType: "personal", models: [{ id: "gpt-test", name: "GPT Test", reasoningLevels: ["medium", "high"], defaultReasoningLevel: "medium" }] });
export const providerReference = (overrides: Partial<ProviderReference> = {}): ProviderReference => ({ id: "ref-1", itemKey: "custom:agent-1", kind: "custom_agent", label: "Analista", details: ["Agente customizado", "Fluxo: Revisão"], choice: { account: "antigo", model: "modelo-anterior", reasoning: null }, ...overrides });
