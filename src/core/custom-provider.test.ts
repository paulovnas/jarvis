import { describe, expect, it } from "vitest";
import { validateCustomProvider, type CustomConfig } from "./custom-provider";
import { customConfigFixture } from "@/test/custom-provider-fixtures";
import { accountList } from "./provider-accounts";
import { customAccountFixture } from "@/test/custom-provider-fixtures";

describe("Custom provider configuration", () => {
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
    expect(validateCustomProvider("provider", config, "key", false)).toContain("menor");
    config.models[0].maxOutputTokens = 1000; config.models.push({ ...config.models[0] });
    expect(validateCustomProvider("provider", config, "key", false)).toContain("únicos");
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
