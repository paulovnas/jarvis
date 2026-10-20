import { expect, it } from "vitest";
import { customConfigSchema, normalizeOllamaBaseUrl, reasoningFormats, validateCustomProvider } from "./custom-provider";
import { accountList, providerKindLabels } from "./provider-accounts";
import { ollamaAccountFixture, customConfigFixture } from "@/test/custom-provider-fixtures";

it("loads an Ollama account with native protocol and thinking controls", () => {
  const account = ollamaAccountFixture();
  expect(customConfigSchema.safeParse(account.custom).success).toBe(true);
  expect(accountList([account])).toEqual([account]);
  expect(providerKindLabels.ollama).toBe("Ollama");
  expect(reasoningFormats("ollama")).toEqual(["none", "toggle", "effort"]);
});

it.each(["", "/", "/v1", "/v1/", "/api", "/api/", "/api/chat", "/api/chat/"])("normalizes the native Ollama host without adding /v1: %s", suffix => {
  expect(normalizeOllamaBaseUrl(` http://192.168.2.102:11434${suffix} `)).toBe("http://192.168.2.102:11434");
});

it.each(["http://localhost:11434", "http://127.0.0.2:11434", "http://[::1]:11434", "http://192.168.2.102:11434", "http://10.20.30.40:11434", "http://172.16.0.1:11434", "http://172.31.255.254:11434", "http://[fd12:3456::1]:11434", "http://[fe80::1]:11434", "https://ollama.example.test"])("accepts a configured Ollama host without an API key: %s", baseUrl => {
  expect(validateCustomProvider("ollama-pessoal", { ...ollamaAccountFixture().custom!, baseUrl }, "", false)).toBeNull();
});

it.each(["http://example.com:11434", "http://8.8.8.8:11434", "http://172.32.0.1:11434", "http://192.169.1.1:11434", "http://169.254.1.1:11434", "http://[2001:db8::1]:11434", "http://localhost.evil.test:11434", "http://10.2.3.evil:11434", "http://192.168.1.1.evil.test:11434", "ftp://localhost:11434", "http://user:secret@localhost:11434", "http://localhost:11434?token=secret", "http://localhost:11434#secret"])("rejects public plaintext hosts and secret-bearing Ollama URLs: %s", baseUrl => {
  expect(validateCustomProvider("ollama-pessoal", { ...ollamaAccountFixture().custom!, baseUrl }, "", false)).not.toBeNull();
});

it("keeps the Custom provider HTTPS restriction and validates Ollama capabilities", () => {
  expect(validateCustomProvider("gateway", { ...customConfigFixture(), baseUrl: "http://192.168.2.102:11434" }, "key", false)).not.toBeNull();
  const config = ollamaAccountFixture().custom!;
  for (const patch of [{ supportsImageGeneration: true }, { reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" }, { supportsChat: false, contextWindow: 0, maxOutputTokens: 0, supportsTools: false, reasoning: "none" as const }]) {
    expect(validateCustomProvider("ollama-pessoal", { ...config, models: [{ ...config.models[0], ...patch }] }, "", false)).not.toBeNull();
  }
});

it("accepts required thinking advertised as on only and rejects an off-only toggle", () => {
  const config = ollamaAccountFixture().custom!;
  config.models[0] = { ...config.models[0], reasoningLevels: ["on"], defaultReasoningLevel: "on" };
  expect(validateCustomProvider("ollama-pessoal", config, "", false)).toBeNull();
  config.models[0] = { ...config.models[0], reasoningLevels: ["off"], defaultReasoningLevel: "off" };
  expect(validateCustomProvider("ollama-pessoal", config, "", false)).not.toBeNull();
});
