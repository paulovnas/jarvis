import { DEFAULT_HTTP_SETTINGS, newHttpRequest, type HttpDraft, type HttpRun, type HttpSnapshot } from "@/core/http-client";

export function httpDraft(patch: Partial<HttpDraft> = {}): HttpDraft {
  return { id: "draft-1", conversationId: "chat-http", projectId: "project-http", revision: 1, savedRequestId: null, request: { ...newHttpRequest(), name: "Consultar pedidos", url: "https://example.test/orders" }, ...patch };
}

export function httpRun(patch: Partial<HttpRun> = {}): HttpRun {
  return { id: "run-1", projectId: "project-http", conversationId: "chat-http", draftId: "draft-1", draftRevision: 1, environmentId: null, environmentRevision: 1, request: httpDraft().request, status: "completed", httpStatus: 200, error: null, outcomeUncertain: false, startedAt: 1_700_000_000_000, finishedAt: 1_700_000_000_123, elapsedMs: 123, receivedBytes: 12, storedBytes: 12, mime: "application/json", url: "https://example.test/orders", headers: [], redirects: [], truncated: false, bodyExpired: false, preview: '{"ok":true}', ...patch };
}

export function httpSnapshot(patch: Partial<HttpSnapshot> = {}): HttpSnapshot {
  return { projectId: "project-http", conversationId: "chat-http", drafts: [httpDraft()], savedRequests: [], runs: [], settings: { ...structuredClone(DEFAULT_HTTP_SETTINGS), projectId: "project-http", revision: 1 }, ...patch };
}
