import { z } from "zod";
import { validateMcpJson } from "./mcp";

const common = { enabled: z.boolean().optional(), timeout: z.number().optional(), requestTimeout: z.number().optional(), request_timeout: z.number().optional() };
const pairs = z.record(z.string(), z.string());
const configSchema = z.discriminatedUnion("type", [
  z.object({ ...common, type: z.literal("local"), command: z.array(z.string()), cwd: z.string().nullish(), environment: pairs.optional() }).strict(),
  z.object({ ...common, type: z.literal("remote"), url: z.string(), headers: pairs.optional(), oauth: z.boolean().nullish() }).strict(),
]);
export type McpPair = { key: string; value: string };
export type McpDraft = {
  name: string; type: "local" | "remote"; command: string; args: string[]; cwd: string;
  environment: McpPair[]; url: string; headers: McpPair[]; enabled: boolean;
  timeout: string; requestTimeout: string; oauth?: boolean | null;
};
export function emptyMcpDraft(): McpDraft {
  return { name: "", type: "local", command: "", args: [], cwd: "", environment: [], url: "", headers: [], enabled: true, timeout: "30000", requestTimeout: "300000" };
}

export function readMcpDraft(raw: string): McpDraft {
  if (!raw.trim()) return emptyMcpDraft();
  const error = validateMcpJson(raw);
  if (error) throw new Error(error);
  const [name, value] = Object.entries(JSON.parse(raw) as Record<string, unknown>)[0];
  const parsed = configSchema.safeParse(value);
  if (!parsed.success) throw new Error("Esta configuração não pode ser exibida no formulário. Confira o tipo e os campos no JSON; seu texto foi preservado.");
  const config = parsed.data;
  const draft = { ...emptyMcpDraft(), name, type: config.type, enabled: config.enabled ?? true, timeout: String(config.timeout ?? 30000), requestTimeout: String(config.requestTimeout ?? config.request_timeout ?? 300000) };
  if (config.requestTimeout !== undefined && config.request_timeout !== undefined) throw new Error("Use apenas requestTimeout ou request_timeout no JSON, não os dois.");
  const entries = (values: Record<string, string> | undefined) => Object.entries(values ?? {}).map(([key, value]) => ({ key, value }));
  return config.type === "local"
    ? { ...draft, command: config.command[0] ?? "", args: config.command.slice(1), cwd: config.cwd ?? "", environment: entries(config.environment) }
    : { ...draft, url: config.url, headers: entries(config.headers), oauth: config.oauth };
}

export function writeMcpDraft(draft: McpDraft, validate = false): string {
  const values = draft.type === "local" ? draft.environment : draft.headers;
  const keys = new Set<string>();
  for (const { key } of values) {
    const normalized = draft.type === "remote" ? key.toLowerCase() : key;
    if (keys.has(normalized)) throw new Error("Há variáveis ou cabeçalhos com nomes repetidos. Corrija os nomes antes de continuar.");
    keys.add(normalized);
  }
  if (validate) {
    if (!/^[a-zA-Z0-9_-]{1,48}$/.test(draft.name)) throw new Error("O nome deve ter até 48 letras, números, hífens ou sublinhados.");
    for (const [label, value, max] of [["Tempo para conectar", draft.timeout, 120000], ["Tempo por chamada", draft.requestTimeout, 900000]] as const) {
      if (!Number.isInteger(Number(value)) || Number(value) < 1000 || Number(value) > max) throw new Error(`${label}: informe entre 1.000 e ${max.toLocaleString("pt-BR")} milissegundos.`);
    }
    if (draft.type === "local") {
      if (!draft.command.trim()) throw new Error("Informe o programa que inicia o MCP.");
      if (draft.args.length > 127 || [draft.command, ...draft.args, draft.cwd].some(value => value.includes("\0"))) throw new Error("Confira o comando, seus argumentos e a pasta de trabalho.");
    } else {
      let url: URL;
      try { url = new URL(draft.url); } catch { throw new Error("Informe uma URL HTTP ou HTTPS válida."); }
      if (!["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password || url.hash) throw new Error("Use uma URL HTTP ou HTTPS sem usuário, senha ou fragmento.");
      if (draft.oauth) throw new Error("OAuth de MCPs ainda não está disponível. Use um cabeçalho de autenticação e oauth: false no JSON.");
    }
    for (const { key, value } of values) {
      if (!key.trim() || key.includes("\0") || value.includes("\0") || (draft.type === "local" && key.includes("="))) throw new Error("Preencha um nome válido para cada variável ou cabeçalho.");
    }
  }
  const common = { enabled: draft.enabled, timeout: Number(draft.timeout), requestTimeout: Number(draft.requestTimeout) };
  const map = (values: McpPair[]) => values.length ? Object.fromEntries(values.map(({ key, value }) => [key, value])) : undefined;
  return JSON.stringify({ [draft.name]: draft.type === "local"
    ? { type: "local", command: [draft.command, ...draft.args], ...(draft.cwd ? { cwd: draft.cwd } : {}), environment: map(draft.environment), ...common }
    : { type: "remote", url: draft.url, headers: map(draft.headers), ...(draft.oauth !== undefined ? { oauth: draft.oauth } : {}), ...common } }, null, 2);
}
