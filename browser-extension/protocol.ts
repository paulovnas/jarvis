import { z } from "zod";

export const pairingSchema = z.object({
  version: z.literal(1),
  endpoint: z.string().refine(value => {
    try {
      const url = new URL(value);
      return url.protocol === "ws:" && url.hostname === "127.0.0.1"
        && Boolean(url.port) && url.pathname === "/extension"
        && !url.username && !url.password && !url.search && !url.hash;
    } catch { return false; }
  }, "O endereço deve apontar para a conexão local do Jarvis."),
  token: z.string().min(32).max(256),
}).strict();
export type Pairing = z.infer<typeof pairingSchema>;

const actionSchema = z.enum([
  "list", "discover", "attach", "open", "select", "close", "navigate", "back", "forward",
  "reload", "snapshot", "console", "screenshot", "click", "fill", "press", "scroll",
  "network", "response_body", "evaluate", "devtools", "prune",
]);
export const requestSchema = z.object({
  type: z.literal("request"), id: z.string().min(1).max(100),
  conversationId: z.string().max(200),
  request: z.object({
    action: actionSchema, id: z.string().max(100).nullish(), url: z.string().max(4096).nullish(),
    newWindow: z.boolean().optional(), element: z.string().max(80).nullish(),
    text: z.string().max(8000).nullish(), key: z.string().max(40).nullish(),
    x: z.number().finite().min(-100000).max(100000).nullish(),
    y: z.number().finite().min(-100000).max(100000).nullish(),
    filter: z.string().max(500).nullish(), offset: z.number().int().min(0).max(10000).nullish(),
    limit: z.number().int().min(1).max(100).nullish(), requestId: z.string().max(300).nullish(),
    expression: z.string().max(32000).nullish(), method: z.string().max(100).nullish(),
    params: z.record(z.string(), z.unknown()).nullish(), retained: z.array(z.string().max(200)).max(10000).optional(),
  }).strict(),
}).strict();
export type Request = z.infer<typeof requestSchema>;

export class BrowserError extends Error {
  constructor(public code: string, message: string) { super(message); }
}

export function address(value: string): string {
  try {
    const url = new URL(value);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error();
    if (["chromewebstore.google.com", "chrome.google.com"].includes(url.hostname)
      && (url.hostname === "chromewebstore.google.com" || url.pathname.startsWith("/webstore"))) throw new Error();
    return url.href;
  } catch { throw new BrowserError("browser_invalid_url", "Use uma página HTTP(S) comum. Páginas internas e a loja de extensões são protegidas pelo navegador."); }
}

export function tabHandle(epoch: string, tabId: number) { return `ext:${epoch}:${tabId}`; }
export function nativeTabId(epoch: string, id?: string | null): number {
  const prefix = `ext:${epoch}:`;
  const suffix = id?.startsWith(prefix) ? id.slice(prefix.length) : "";
  if (!/^\d+$/.test(suffix) || !Number.isSafeInteger(Number(suffix))) {
    throw new BrowserError("browser_stale_tab", "A aba pertence a outra sessão do navegador. Liste ou selecione uma aba novamente.");
  }
  return Number(suffix);
}

const pageDomains = new Set(["Accessibility", "CSS", "DOM", "DOMSnapshot", "Emulation", "Input", "Log", "Network", "Page", "Performance", "Runtime"]);
export function validateMethod(method: string, params: Record<string, unknown>): void {
  if (!/^[A-Za-z]+\.[A-Za-z]+$/.test(method) || !pageDomains.has(method.split(".")[0])
    || /Cookie|DownloadBehavior|Permissions|BrowserContext|crash|close|terminateExecution/i.test(method)) {
    throw new BrowserError("browser_unsupported_method", "Este comando CDP não pertence à página controlada. Use as operações de abas do Jarvis.");
  }
  if (method === "Page.navigate") address(String(params.url ?? ""));
  if (method === "Page.navigate" && params.frameId) {
    throw new BrowserError("browser_unsupported_method", "Navegue pela aba controlada sem sobrescrever frameId.");
  }
  if (method === "DOM.setFileInputFiles" || method === "Page.addScriptToEvaluateOnNewDocument") {
    throw new BrowserError("browser_unsupported_method", "Este comando exige acesso persistente ou a arquivos locais e não está disponível nesta integração.");
  }
}

export function object(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {};
}
export function bounded(value: unknown, limit = 48000): unknown {
  const text = JSON.stringify(value ?? null);
  return text.length <= limit ? value ?? null : { text: text.slice(0, limit), truncated: true, totalCharacters: text.length };
}

export function errorResult(error: unknown) {
  return error instanceof BrowserError ? { code: error.code, message: error.message }
    : { code: "browser_command_failed", message: error instanceof Error ? error.message.slice(0, 1000) : "Falha na operação do navegador." };
}

export class CommandQueue {
  private tails = new Map<number, Promise<unknown>>();
  run<T>(key: number, operation: () => Promise<T>): Promise<T> {
    const previous = this.tails.get(key) ?? Promise.resolve();
    const result = previous.catch(() => {}).then(operation);
    this.tails.set(key, result);
    void result.finally(() => { if (this.tails.get(key) === result) this.tails.delete(key); }).catch(() => {});
    return result;
  }
}
