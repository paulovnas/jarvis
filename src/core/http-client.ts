import { z } from "zod";

export const httpPairSchema = z.object({ name: z.string(), value: z.string(), enabled: z.boolean() });
export const httpVariableSchema = httpPairSchema.extend({ id: z.string(), secret: z.boolean(), configured: z.boolean() });
export const httpEnvironmentSchema = z.object({ id: z.string(), name: z.string(), color: z.string(), variables: httpVariableSchema.array() });
export const httpDefaultsSchema = z.object({ connectTimeoutSeconds: z.number(), readTimeoutSeconds: z.number(), totalTimeoutSeconds: z.number().nullable(), maxResponseBytes: z.number(), historyLimit: z.number(), followRedirects: z.boolean(), verifyTls: z.boolean(), proxyUrl: z.string(), caFile: z.string() });
export const httpSettingsSchema = z.object({ projectId: z.string(), revision: z.number(), variables: httpVariableSchema.array(), environments: httpEnvironmentSchema.array(), defaults: httpDefaultsSchema });
export const httpAuthSchema = z.object({ type: z.enum(["none", "basic", "bearer", "apiKey"]), username: z.string(), password: z.string(), token: z.string(), name: z.string(), value: z.string(), location: z.enum(["header", "query"]) });
export const httpBodyFieldSchema = httpPairSchema.extend({ fileId: z.string().nullable() });
export const httpBodySchema = z.object({ type: z.enum(["none", "json", "text", "urlencoded", "multipart", "binary"]), text: z.string(), fields: httpBodyFieldSchema.array(), fileId: z.string().nullable() });
export const httpRequestSchema = z.object({ name: z.string(), method: z.string(), url: z.string(), environmentId: z.string().nullable(), params: httpPairSchema.array(), headers: httpPairSchema.array(), auth: httpAuthSchema, body: httpBodySchema });
export const httpSavedRequestSchema = z.object({ id: z.string(), projectId: z.string(), revision: z.number(), request: httpRequestSchema });
export const httpDraftSchema = z.object({ id: z.string(), conversationId: z.string(), projectId: z.string(), revision: z.number(), savedRequestId: z.string().nullable(), request: httpRequestSchema });
export const httpRunSchema = z.object({ id: z.string(), projectId: z.string(), conversationId: z.string(), draftId: z.string(), draftRevision: z.number(), environmentId: z.string().nullable(), environmentRevision: z.number(), request: httpRequestSchema, status: z.enum(["running", "completed", "failed", "cancelled", "interrupted"]), httpStatus: z.number().nullable(), error: z.string().nullable(), outcomeUncertain: z.boolean(), startedAt: z.number(), finishedAt: z.number().nullable(), elapsedMs: z.number(), receivedBytes: z.number(), storedBytes: z.number(), mime: z.string(), url: z.string(), headers: httpPairSchema.array(), redirects: z.string().array(), truncated: z.boolean(), bodyExpired: z.boolean(), preview: z.string() });
export const httpSnapshotSchema = z.object({ projectId: z.string(), conversationId: z.string(), drafts: httpDraftSchema.array(), savedRequests: httpSavedRequestSchema.array(), runs: httpRunSchema.array(), settings: httpSettingsSchema });
export const httpResultSchema = z.object({ run: httpRunSchema, text: z.string(), offset: z.number(), nextOffset: z.number().nullable(), totalBytes: z.number(), binary: z.boolean() });
export const httpFileSchema = z.object({ id: z.string(), name: z.string(), size: z.number() });

export type HttpPair = z.infer<typeof httpPairSchema>;
export type HttpVariable = z.infer<typeof httpVariableSchema>;
export type HttpEnvironment = z.infer<typeof httpEnvironmentSchema>;
export type HttpDefaults = z.infer<typeof httpDefaultsSchema>;
export type HttpSettings = z.infer<typeof httpSettingsSchema>;
export type HttpRequest = z.infer<typeof httpRequestSchema>;
export type HttpDraft = z.infer<typeof httpDraftSchema>;
export type HttpSavedRequest = z.infer<typeof httpSavedRequestSchema>;
export type HttpRun = z.infer<typeof httpRunSchema>;
export type HttpSnapshot = z.infer<typeof httpSnapshotSchema>;
export type HttpResult = z.infer<typeof httpResultSchema>;
export type HttpFile = z.infer<typeof httpFileSchema>;

export const DEFAULT_HTTP_SETTINGS: Omit<HttpSettings, "projectId" | "revision"> = { variables: [], environments: [], defaults: { connectTimeoutSeconds: 10, readTimeoutSeconds: 30, totalTimeoutSeconds: null, maxResponseBytes: 10 * 1024 * 1024, historyLimit: 100, followRedirects: false, verifyTls: true, proxyUrl: "", caFile: "" } };
export function newHttpRequest(): HttpRequest { return { name: "Nova requisição", method: "GET", url: "", environmentId: null, params: [], headers: [], auth: { type: "none", username: "", password: "", token: "", name: "", value: "", location: "header" }, body: { type: "none", text: "", fields: [], fileId: null } }; }

// IPC contract (all responses are validated with the schemas above):
// get_project_http_settings({projectId}) -> HttpSettings
// save_project_http_settings({projectId, settings: HttpSettings}) -> HttpSettings
// get_http_snapshot({conversationId}) -> HttpSnapshot
// save_http_draft({conversationId, id: string|null, revision: number, request: HttpRequest, savedRequestId: string|null}) -> HttpDraft
// close_http_draft({conversationId, id, revision}) -> void (active runs remain in history)
// save_http_request({projectId, id: string|null, revision, request}) -> HttpSavedRequest
// delete_http_request({projectId, id, revision}) -> void
// send_http_request({conversationId, draftId, revision}) -> HttpRun (running, no implicit retries)
// cancel_http_request({conversationId, runId}) -> void
// get_http_result({conversationId, runId, offset?:number, limit?:number}) -> HttpResult
// save_http_response({conversationId, runId, path}) -> void (explicit raw export)
// import_http_file({projectId, path}) -> HttpFile (UI-selected file, copied and bounded)
// export_project_http({projectId, path}) -> void (no secrets/results/file bytes)
// import_project_http({projectId, path, revision}) -> HttpSettings (merge requests, replace settings)
// http:changed event -> {conversationId: string, runId?: string}
