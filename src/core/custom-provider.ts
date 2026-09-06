import { z } from "zod";

export const protocolLabels = {
  "openai-completions": "OpenAI · Chat Completions",
  "openai-responses": "OpenAI · Responses",
  "anthropic-messages": "Anthropic · Messages",
} as const;
export const reasoningLabels = {
  none: "Padrão do provedor",
  effort: "Nível de esforço",
  openrouter: "OpenRouter · reasoning",
  deepseek: "DeepSeek · thinking",
  budget: "Thinking · orçamento de tokens",
  adaptive: "Thinking · adaptativo",
} as const;
export const customModelSchema = z.object({
  id: z.string().min(1).max(200).regex(/^\S+$/),
  name: z.string().trim().min(1).max(120),
  contextWindow: z.number().int().min(4096).max(100_000_000),
  maxOutputTokens: z.number().int().positive().max(10_000_000),
  supportsImages: z.boolean(), supportsTools: z.boolean(),
  reasoning: z.enum(["none", "effort", "openrouter", "deepseek", "budget", "adaptive"]),
  reasoningLevels: z.array(z.string().regex(/^[a-z0-9_-]{1,32}$/)).max(12),
  defaultReasoningLevel: z.string().nullable(),
  thinkingBudget: z.number().int().min(1024).nullable(),
});
export const customConfigSchema = z.object({
  baseUrl: z.string().url().max(2048),
  protocol: z.enum(["openai-completions", "openai-responses", "anthropic-messages"]),
  authMode: z.enum(["bearer", "x-api-key"]),
  tokenField: z.enum(["max_tokens", "max_completion_tokens"]),
  replayUnsignedThinking: z.boolean().default(false),
  models: z.array(customModelSchema).min(1).max(100),
});
export type CustomConfig = z.infer<typeof customConfigSchema>;
export type CustomModel = z.infer<typeof customModelSchema>;
export const discoveredModelSchema = z.object({ model: customModelSchema, sourceUrl: z.string().url(), tokenField: z.enum(["max_tokens", "max_completion_tokens"]) });
export function supportsModelLookup(baseUrl: string): boolean {
  try {
    const url = new URL(baseUrl);
    return url.protocol === "https:" && url.hostname === "openrouter.ai" && (!url.port || url.port === "443") && !url.username && !url.password && !url.search && !url.hash && /^\/api\/v1(?:\/chat\/completions|\/responses|\/messages)?\/?$/.test(url.pathname);
  } catch { return false; }
}
export function reasoningFormats(protocol: CustomConfig["protocol"]): CustomModel["reasoning"][] {
  return protocol === "anthropic-messages" ? ["none", "budget", "adaptive"] : protocol === "openai-responses" ? ["none", "effort"] : ["none", "effort", "openrouter", "deepseek"];
}
export function validateCustomProvider(alias: string, config: CustomConfig, apiKey: string, editing: boolean): string | null {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,63}$/.test(alias)) return "Alias: use até 64 letras, números, pontos, hífens ou sublinhados.";
  if (!editing && !apiKey.trim()) return "Informe a chave de API.";
  if (apiKey && ([...apiKey].some(char => char.charCodeAt(0) < 33 || char.charCodeAt(0) > 126) || apiKey.length > 8192)) return "A chave contém caracteres inválidos.";
  if (!customConfigSchema.safeParse(config).success) return "Preencha URL, ID, nome, contexto (mínimo 4.096 tokens) e saída de cada modelo.";
  const url = new URL(config.baseUrl);
  if ((url.protocol !== "https:" && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))) || url.username || url.password || url.search || url.hash) return "Use HTTPS (HTTP apenas local), sem credenciais ou parâmetros na URL.";
  if (new Set(config.models.map(m => m.id)).size !== config.models.length) return "Os IDs dos modelos precisam ser únicos.";
  for (const model of config.models) {
    if (model.maxOutputTokens >= model.contextWindow) return `${model.name}: a saída deve ser menor que a janela de contexto.`;
    if (!reasoningFormats(config.protocol).includes(model.reasoning)) return `${model.name}: formato de raciocínio incompatível com o endpoint.`;
    if (model.reasoning !== "none" && (!model.reasoningLevels.length || !model.defaultReasoningLevel || !model.reasoningLevels.includes(model.defaultReasoningLevel))) return `${model.name}: informe os níveis aceitos e selecione o padrão.`;
    if (new Set(model.reasoningLevels).size !== model.reasoningLevels.length) return `${model.name}: remova níveis repetidos.`;
    if (model.reasoning === "budget" && (model.thinkingBudget === null || model.thinkingBudget >= model.maxOutputTokens)) return `${model.name}: o orçamento de thinking deve ser menor que a saída.`;
  }
  return null;
}
