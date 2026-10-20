import { z } from "zod";

export const protocolLabels = {
  "openai-completions": "OpenAI · Chat Completions",
  "openai-responses": "OpenAI · Responses",
  "anthropic-messages": "Anthropic · Messages",
  ollama: "Ollama · API nativa",
} as const;
export const reasoningLabels = {
  none: "Padrão do provedor",
  effort: "Nível de esforço",
  openrouter: "OpenRouter · reasoning",
  deepseek: "DeepSeek · thinking",
  budget: "Thinking · orçamento de tokens",
  adaptive: "Thinking · adaptativo",
  toggle: "Thinking · ativar/desativar",
} as const;
export const customModelIdSchema = z.string().min(1).max(200).refine(id => id.trim().length > 0 && [...id].every(char => { const code = char.charCodeAt(0); return code >= 32 && (code < 127 || code > 159); }), "Informe o ID literal, sem caracteres de controle.");
export const customModelSchema = z.object({
  id: customModelIdSchema,
  name: z.string().trim().min(1).max(120),
  contextWindow: z.number().int().min(0).max(100_000_000),
  maxOutputTokens: z.number().int().min(0).max(10_000_000),
  supportsChat: z.boolean().default(true),
  supportsImageGeneration: z.boolean().default(false),
  supportsImages: z.boolean(), supportsTools: z.boolean(),
  reasoning: z.enum(["none", "effort", "openrouter", "deepseek", "budget", "adaptive", "toggle"]),
  reasoningLevels: z.array(z.string().regex(/^[a-z0-9_-]{1,32}$/)).max(12),
  defaultReasoningLevel: z.string().nullable(),
  thinkingBudget: z.number().int().min(1024).nullable(),
}).refine(model => model.supportsChat ? model.contextWindow >= 4096 && model.maxOutputTokens > 0 : !model.supportsTools && model.reasoning === "none", "Informe os limites do chat ou desabilite os recursos de chat deste modelo.");
export const customConfigSchema = z.object({
  baseUrl: z.string().url().max(2048),
  protocol: z.enum(["openai-completions", "openai-responses", "anthropic-messages", "ollama"]),
  authMode: z.enum(["bearer", "x-api-key"]),
  tokenField: z.enum(["max_tokens", "max_completion_tokens"]),
  replayUnsignedThinking: z.boolean().default(false),
  models: z.array(customModelSchema).min(1).max(100),
});
export type CustomConfig = z.infer<typeof customConfigSchema>;
export type CustomModel = z.infer<typeof customModelSchema>;
export const customCatalogModelSchema = z.object({
  id: customModelIdSchema,
  name: z.string().min(1).max(120),
  contextWindow: z.number().nullable(),
  maxOutputTokens: z.number().nullable(),
  supportsTools: z.boolean().nullable(),
  supportsImages: z.boolean().nullable(),
  reasoningLevels: z.array(z.object({ value: customModelSchema.shape.reasoningLevels.element, display: z.string().min(1).max(120) })).default([]),
  defaultReasoningLevel: z.string().nullable().default(null),
});
export const customCatalogSchema = z.object({ models: z.array(customCatalogModelSchema).max(10_000) });
export type CustomCatalogModel = z.infer<typeof customCatalogModelSchema>;
export function normalizeCustomBaseUrl(baseUrl: string): string {
  try {
    const url = new URL(baseUrl);
    if (url.pathname.replace(/\/+$/, "")) return baseUrl;
    url.pathname = "/v1";
    return url.toString();
  } catch { return baseUrl; }
}
export function normalizeOllamaBaseUrl(baseUrl: string): string {
  try {
    const url = new URL(baseUrl.trim());
    url.pathname = url.pathname.replace(/\/+$/, "").replace(/\/(?:v1|api(?:\/chat)?)$/, "");
    return url.toString().replace(/\/$/, "");
  } catch { return baseUrl.trim(); }
}
export function isOllamaBaseUrl(baseUrl: string): boolean {
  try {
    const url = new URL(baseUrl);
    if (url.username || url.password || url.search || url.hash) return false;
    if (url.protocol === "https:") return true;
    if (url.protocol !== "http:") return false;
    const host = url.hostname;
    if (["localhost", "[::1]"].includes(host) || /^\[f[cd][a-f0-9]{2}:/.test(host) || /^\[fe[89ab][a-f0-9]:/.test(host)) return true;
    const parts = host.split(".").map(Number);
    return parts.length === 4 && parts.every(part => Number.isInteger(part) && part >= 0 && part <= 255) && (parts[0] === 127 || parts[0] === 10 || parts[0] === 172 && parts[1] >= 16 && parts[1] <= 31 || parts[0] === 192 && parts[1] === 168);
  } catch { return false; }
}
export const discoveredModelSchema = z.object({ model: customModelSchema, sourceUrl: z.string().url(), tokenField: z.enum(["max_tokens", "max_completion_tokens"]) });
export function supportsModelLookup(baseUrl: string): boolean {
  try {
    const url = new URL(baseUrl);
    return url.protocol === "https:" && url.hostname === "openrouter.ai" && (!url.port || url.port === "443") && !url.username && !url.password && !url.search && !url.hash && /^\/api\/v1(?:\/chat\/completions|\/responses|\/messages)?\/?$/.test(url.pathname);
  } catch { return false; }
}
export function reasoningFormats(protocol: CustomConfig["protocol"]): CustomModel["reasoning"][] {
  return protocol === "ollama" ? ["none", "toggle", "effort"] : protocol === "anthropic-messages" ? ["none", "budget", "adaptive"] : protocol === "openai-responses" ? ["none", "effort"] : ["none", "effort", "openrouter", "deepseek"];
}
export function validateCustomProvider(alias: string, config: CustomConfig, apiKey: string, editing: boolean): string | null {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,63}$/.test(alias)) return "Alias: use até 64 letras, números, pontos, hífens ou sublinhados.";
  if (config.protocol !== "ollama" && !editing && !apiKey.trim()) return "Informe a chave de API.";
  if (apiKey && ([...apiKey].some(char => char.charCodeAt(0) < 33 || char.charCodeAt(0) > 126) || apiKey.length > 8192)) return "A chave contém caracteres inválidos.";
  if (!customConfigSchema.safeParse(config).success) return "Preencha URL, ID, nome, contexto (mínimo 4.096 tokens) e saída de cada modelo.";
  const url = new URL(config.baseUrl);
  if (config.protocol === "ollama" ? !isOllamaBaseUrl(config.baseUrl) : (url.protocol !== "https:" && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))) || url.username || url.password || url.search || url.hash) return "Use HTTPS (HTTP apenas local ou, no Ollama, em rede privada), sem credenciais ou parâmetros na URL.";
  if (new Set(config.models.map(m => m.id)).size !== config.models.length) return "Os IDs dos modelos precisam ser únicos.";
  for (const model of config.models) {
    if (config.protocol === "ollama" && (!model.supportsChat || model.supportsImageGeneration)) return "O Ollama usa modelos de chat; imagens de entrada não habilitam geração de imagens.";
    if (model.supportsImageGeneration && (!supportsModelLookup(config.baseUrl) || config.authMode !== "bearer")) return "A geração de imagens em provedores Custom está disponível para o OpenRouter com autenticação Bearer.";
    if (!reasoningFormats(config.protocol).includes(model.reasoning)) return `${model.name}: formato de raciocínio incompatível com o endpoint.`;
    if (model.reasoning !== "none" && (!model.reasoningLevels.length || !model.defaultReasoningLevel || !model.reasoningLevels.includes(model.defaultReasoningLevel))) return `${model.name}: informe os níveis aceitos e selecione o padrão.`;
    if (new Set(model.reasoningLevels).size !== model.reasoningLevels.length) return `${model.name}: remova níveis repetidos.`;
    if (model.reasoning === "toggle" && (!model.reasoningLevels.includes("on") || model.reasoningLevels.some(level => level !== "off" && level !== "on"))) return `${model.name}: use on e inclua off apenas se o modelo aceitar.`;
    if (model.reasoning === "budget" && (model.thinkingBudget === null || model.thinkingBudget >= model.maxOutputTokens)) return `${model.name}: o orçamento de thinking deve ser menor que a saída.`;
  }
  return null;
}
