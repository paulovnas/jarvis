import type { ProviderAccount } from "@/core/provider-accounts";
import type { CustomConfig } from "@/core/custom-provider";

export function customConfigFixture(): CustomConfig {
  return { baseUrl: "https://gateway.example/api/v1", protocol: "openai-completions", authMode: "bearer", tokenField: "max_tokens", replayUnsignedThinking: false, models: [{ id: "vendor/model", name: "Meu modelo", contextWindow: 64_000, maxOutputTokens: 4000, supportsChat: true, supportsImageGeneration: false, supportsImages: true, supportsTools: true, reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null, thinkingBudget: null }] };
}
export function customAccountFixture(): ProviderAccount {
  const custom = customConfigFixture();
  return { alias: "Minha.Gateway", providerKind: "custom", enabled: true, showUsage: false, createdAt: 1, accountType: "unknown", email: null, models: custom.models.map(model => ({ ...model })), modelsAvailable: true, custom };
}

export function ollamaAccountFixture(): ProviderAccount {
  const custom: CustomConfig = { ...customConfigFixture(), baseUrl: "http://192.168.2.102:11434", protocol: "ollama", models: [
    { ...customConfigFixture().models[0], id: "qwen3:14b", name: "qwen3:14b", contextWindow: 8192, maxOutputTokens: 4096, supportsImages: false, reasoning: "toggle", reasoningLevels: ["off", "on"], defaultReasoningLevel: "on" },
  ] };
  return { ...customAccountFixture(), alias: "ollama-pessoal", displayName: "Ollama", providerKind: "ollama", models: custom.models.map(model => ({ ...model })), custom };
}
