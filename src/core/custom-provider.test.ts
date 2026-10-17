import { describe, expect, it } from "vitest";
import { customCatalogModelSchema, customModelSchema, normalizeCustomBaseUrl, validateCustomProvider, type CustomConfig } from "./custom-provider";
import { customConfigFixture } from "@/test/custom-provider-fixtures";
import { accountList } from "./provider-accounts";
import { customAccountFixture } from "@/test/custom-provider-fixtures";

describe("Custom provider configuration", () => {
  it("completa somente novas URLs sem caminho e preserva versões e rotas explícitas", () => {
    expect(normalizeCustomBaseUrl("https://gateway.example/")).toBe("https://gateway.example/v1");
    for (const url of ["https://gateway.example/v1", "https://gateway.example/api/v2/", "http://localhost:8080/chat/completions", "not a URL"]) expect(normalizeCustomBaseUrl(url)).toBe(url);
  });
  it("preserva IDs literais nos três protocolos e rejeita controles e IDs vazios", () => {
    for (const protocol of Object.keys({ "openai-completions": true, "openai-responses": true, "anthropic-messages": true }) as CustomConfig["protocol"][]) {
      for (const id of ["Opus 5.5", "GPT-5.6", "vendor/Meu Modelo-4.7", "Opus 5", "opus 5"]) {
        const config = customConfigFixture(); config.protocol = protocol; config.models[0].id = id;
        expect(validateCustomProvider("gateway", config, "key", false)).toBeNull();
        expect(customModelSchema.parse(config.models[0]).id).toBe(id);
      }
    }
    for (const id of ["", "   ", "model\nname", "model\tname", "model\u0000name"]) {
      expect(customModelSchema.safeParse({ ...customConfigFixture().models[0], id }).success).toBe(false);
    }
  });
  it("preserva o alias completo e modelos com namespace no catálogo", () => {
    const account = customAccountFixture();
    expect(accountList([account])).toEqual([account]);
    expect(validateCustomProvider("Minha.Gateway", account.custom!, "key", false)).toBeNull();
    expect(validateCustomProvider("invalid/alias", account.custom!, "key", false)).toContain("Alias");
  });
  it("exige limites explícitos e coerentes antes de salvar", () => {
    const config = customConfigFixture();
    config.models[0].contextWindow = 0;
    expect(validateCustomProvider("provider", config, "key", false)).toContain("contexto");
    config.models[0].contextWindow = 4000;
    expect(validateCustomProvider("provider", config, "key", false)).not.toBeNull();
    config.models[0].contextWindow = 4096; config.models[0].maxOutputTokens = 4096;
    expect(validateCustomProvider("provider", config, "key", false)).toBeNull();
    config.models[0].maxOutputTokens = 8192;
    expect(validateCustomProvider("provider", config, "key", false)).toBeNull();
    config.models[0].maxOutputTokens = 1000; config.models.push({ ...config.models[0] });
    expect(validateCustomProvider("provider", config, "key", false)).toContain("únicos");
  });
  it("mantém limites inválidos do catálogo editáveis e aceita catálogos antigos sem raciocínio", () => {
    const model = customCatalogModelSchema.parse({ id: "literal model", name: "Literal", contextWindow: -1, maxOutputTokens: 10_000_001, supportsTools: null, supportsImages: null });
    expect(model).toEqual({ id: "literal model", name: "Literal", contextWindow: -1, maxOutputTokens: 10_000_001, supportsTools: null, supportsImages: null, reasoningLevels: [], defaultReasoningLevel: null });
  });
  it("aceita os três endpoints e mantém a chave ao editar", () => {
    for (const protocol of ["openai-completions", "openai-responses", "anthropic-messages"] as CustomConfig["protocol"][]) {
      const config = { ...customConfigFixture(), protocol };
      expect(validateCustomProvider("provider", config, "", false)).toContain("chave");
      expect(validateCustomProvider("provider", config, "", true)).toBeNull();
    }
  });
  it("rejeita raciocínio incompatível e segredo na URL", () => {
    const config = customConfigFixture(); config.models[0].reasoning = "budget";
    expect(validateCustomProvider("provider", config, "key", false)).toContain("incompatível");
    config.models[0].reasoning = "effort";
    expect(validateCustomProvider("provider", config, "key", false)).toContain("padrão");
    config.baseUrl = "https://example.com/v1?api_key=secret";
    expect(validateCustomProvider("provider", config, "key", false)).toContain("parâmetros");
  });
});
