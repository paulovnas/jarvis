import type { ProviderAccount } from "@/core/provider-accounts";
import type { CustomConfig } from "@/core/custom-provider";

export function customConfigFixture(): CustomConfig {
  return { baseUrl: "https://gateway.example/api/v1", protocol: "openai-completions", authMode: "bearer", tokenField: "max_tokens", replayUnsignedThinking: false, models: [{ id: "vendor/model", name: "Meu modelo", contextWindow: 64_000, maxOutputTokens: 4000, supportsImages: true, supportsTools: true, reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null, thinkingBudget: null }] };
}
export function customAccountFixture(): ProviderAccount {
  const custom = customConfigFixture();
  return { alias: "Minha.Gateway", providerKind: "custom", enabled: true, showUsage: false, createdAt: 1, accountType: "unknown", email: null, models: custom.models, modelsAvailable: true, custom };
}
