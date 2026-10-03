import { z } from "zod";
import { historyPageSchema, readChat, turnOptionsSchema, type ChatSnapshot, type HistoryPage } from "@/core/chat";
import { readLibrarySnapshot, type LibrarySnapshot } from "@/core/library";
import { workflowSchema, type WorkflowSnapshot } from "@/core/workflow";
import { companionModelsSchema } from "@/core/companion";
import { accountUsageSchema } from "@/core/provider-usage";
import { modelChoiceSchema, workflowCatalogSchema } from "@/core/workflow-catalog";
import type { ModelChoice } from "@/core/provider-references";

const sessionSchema = z.object({ deviceId: z.string().min(1), name: z.string().min(1) });
const envelopeSchema = z.discriminatedUnion("ok", [
  z.object({ ok: z.literal(true), data: z.unknown() }),
  z.object({ ok: z.literal(false), error: z.object({ code: z.string(), message: z.string() }) }),
]);
export const runtimeSchema = z.object({
  conversationId: z.string(), revision: z.number().int().nonnegative(), activeTurnId: z.string().nullable(), compacting: z.boolean(),
  status: z.enum(["running", "waiting", "idle", "completed", "failed", "blocked", "cancelled", "interrupted"]).optional(),
  attention: z.array(z.object({ kind: z.enum(["question", "approval", "publication", "authoring", "validation"]), agentId: z.string(), turnId: z.string().optional(), toolId: z.string().optional(), batchId: z.string().optional() })),
});
const libraryResultSchema = z.object({ library: z.unknown(), runtime: z.array(runtimeSchema), discoveringAttention: z.boolean().optional(), attentionDiscoveryFailed: z.boolean().optional() });
const chatResultSchema = z.object({ chat: z.unknown(), workflow: workflowSchema.nullable(), options: turnOptionsSchema.nullable() });
const beadsResultSchema = z.object({ issues: z.array(z.object({
  id: z.string().min(1), title: z.string(), status: z.string(), issueType: z.string(), parentId: z.string().nullable(),
})) });
const modelSettingsSchema = z.record(z.string(), modelChoiceSchema);
export const remoteChoicesSchema = z.object({ models: companionModelsSchema, catalog: workflowCatalogSchema, defaults: modelSettingsSchema, overrides: modelSettingsSchema });
export const remoteUsageSchema = z.array(accountUsageSchema.extend({ providerKind: z.enum(["openai-codex", "antigravity", "claude-code", "opencode-go"]) }));
export type RemoteSession = z.infer<typeof sessionSchema>;
export type RemoteRuntime = z.infer<typeof runtimeSchema>;
export interface RemoteLibrary { library: LibrarySnapshot; runtime: RemoteRuntime[]; discoveringAttention?: boolean; attentionDiscoveryFailed?: boolean }
export interface RemoteChat { chat: ChatSnapshot; workflow: WorkflowSnapshot | null; options: z.infer<typeof turnOptionsSchema> | null }
export type RemoteBeads = z.infer<typeof beadsResultSchema>;
export type RemoteChoices = z.infer<typeof remoteChoicesSchema>;
export type RemoteUsageAccount = z.infer<typeof remoteUsageSchema>[number];
export type Mutation = "message" | "question" | "approval" | "validation" | "authoring" | "cancel" | "queue_edit" | "queue_delete" | "queue_send_now" | "chat_model";

export class RemoteError extends Error {
  constructor(public code: string, message: string, public status = 0) { super(message); this.name = "RemoteError"; }
  get expired() { return this.code === "session_expired" || this.status === 401; }
}

/** Consume the credential before rendering or fetching anything. Keep it only in memory. */
export function takePairingToken(location: Pick<Location, "hash" | "pathname" | "search">, history: Pick<History, "replaceState">): string | null {
  const fragment = new URLSearchParams(location.hash.replace(/^#/, ""));
  const token = fragment.get("pair");
  if (fragment.has("pair")) {
    fragment.delete("pair");
    const remaining = fragment.toString();
    history.replaceState(null, "", `${location.pathname}${location.search}${remaining ? `#${remaining}` : ""}`);
  }
  return token;
}

function requestId(): string {
  // LAN HTTP is not a secure context on every phone, so randomUUID may be absent.
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), byte => byte.toString(16).padStart(2, "0")).join("");
}

export class RemoteClient {
  constructor(private fetcher: typeof fetch = (...args) => fetch(...args), private timeoutMs = 15_000) {}

  private async request(path: string, body?: unknown, signal?: AbortSignal): Promise<unknown> {
    const controller = new AbortController();
    const abort = () => controller.abort();
    if (signal?.aborted) controller.abort();
    signal?.addEventListener("abort", abort, { once: true });
    const timeout = setTimeout(abort, this.timeoutMs);
    try {
      const response = await this.fetcher(path, {
        method: body === undefined ? "GET" : "POST", credentials: "same-origin", cache: "no-store", signal: controller.signal,
        headers: { Accept: "application/json", ...(body === undefined ? {} : { "Content-Type": "application/json" }) },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      const envelope = envelopeSchema.safeParse(await response.json());
      if (!envelope.success) throw new RemoteError("invalid_response", "O computador enviou uma resposta inválida.", response.status);
      if (!envelope.data.ok) throw new RemoteError(envelope.data.error.code, envelope.data.error.message, response.status);
      if (!response.ok) throw new RemoteError("http_error", "Não foi possível acessar o computador.", response.status);
      return envelope.data.data;
    } catch (error) {
      if (error instanceof RemoteError) throw error;
      if (controller.signal.aborted) throw new RemoteError("timeout", "O computador não respondeu a tempo. Atualize o estado antes de repetir uma ação.");
      throw new RemoteError("offline", "Não foi possível conectar ao computador. Verifique a rede.");
    } finally {
      clearTimeout(timeout);
      signal?.removeEventListener("abort", abort);
    }
  }

  async session(signal?: AbortSignal): Promise<RemoteSession> { return sessionSchema.parse(await this.request("/api/session", undefined, signal)); }
  async pair(token: string, name: string): Promise<RemoteSession> { return sessionSchema.parse(await this.request("/api/pair", { token, name })); }
  async logout(): Promise<void> { await this.request("/api/logout", {}); }
  async library(signal?: AbortSignal): Promise<RemoteLibrary> {
    const result = libraryResultSchema.parse(await this.request("/api/rpc", { method: "library", params: {} }, signal));
    return { library: readLibrarySnapshot(result.library), runtime: result.runtime, discoveringAttention: result.discoveringAttention, attentionDiscoveryFailed: result.attentionDiscoveryFailed };
  }
  async chat(conversationId: string, signal?: AbortSignal): Promise<RemoteChat> {
    const result = chatResultSchema.parse(await this.request("/api/rpc", { method: "chat", params: { conversationId } }, signal));
    return { ...result, chat: readChat(result.chat, conversationId) };
  }
  async beads(conversationId: string, signal?: AbortSignal): Promise<RemoteBeads> {
    return beadsResultSchema.parse(await this.request("/api/rpc", { method: "beads", params: { conversationId } }, signal));
  }
  async choices(conversationId: string, signal?: AbortSignal): Promise<RemoteChoices> {
    return remoteChoicesSchema.parse(await this.request("/api/rpc", { method: "choices", params: { conversationId } }, signal));
  }
  async usage(refresh = true, signal?: AbortSignal): Promise<RemoteUsageAccount[]> {
    return remoteUsageSchema.parse(await this.request("/api/rpc", { method: "usage", params: { refresh } }, signal));
  }
  async setChatModel(conversationId: string, key: string, choice: ModelChoice) {
    return modelSettingsSchema.parse(await this.mutate("chat_model", { conversationId, key, choice }));
  }
  async history(conversationId: string, before: number): Promise<HistoryPage> {
    const result = historyPageSchema.parse(await this.request("/api/rpc", { method: "history", params: { conversationId, before } }));
    if (result.conversationId !== conversationId) throw new RemoteError("invalid_response", "O histórico recebido pertence a outra conversa.");
    return result;
  }
  async mutate(method: Mutation, params: Record<string, unknown>): Promise<unknown> {
    // Never retry mutations. A lost reply may still have changed the shared runtime.
    return this.request("/api/rpc", { method, params, requestId: requestId() });
  }
}
