import { afterEach, expect, it, vi } from "vitest";
import { RemoteClient, RemoteError, takePairingToken } from "./client";
import { customCatalog } from "@/test/workflow-fixtures";

it("reads sanitized selection and usage data through authenticated RPC", async () => {
  const choices = { models: [], catalog: customCatalog, defaults: {}, overrides: {} };
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async (_url, init) => {
    const { method } = JSON.parse(String(init?.body)) as { method: string };
    return response(method === "choices" ? choices : []);
  });
  const client = new RemoteClient(fetcher);
  expect(await client.choices("c1")).toEqual(choices);
  expect(await client.usage(true)).toEqual([]);
  const bodies = fetcher.mock.calls.map(call => JSON.parse(String(call[1]?.body)) as { method: string; params: unknown; requestId?: string });
  expect(bodies).toEqual([{ method: "choices", params: { conversationId: "c1" } }, { method: "usage", params: { refresh: true } }]);
  expect(fetcher.mock.calls.every(call => call[1]?.credentials === "same-origin")).toBe(true);
});

it("saves a chat model once with the idempotent mutation contract", async () => {
  const choice = { account: "ene", model: "sol", reasoning: "high", fallback: { executor: "claude" as const, account: "", model: "sonnet", reasoning: null } };
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response({ "standard/builder": choice }));
  const client = new RemoteClient(fetcher);
  expect(await client.setChatModel("c1", "standard/builder", choice)).toEqual({ "standard/builder": choice });
  const body = JSON.parse(String(fetcher.mock.calls[0][1]?.body)) as { method: string; params: unknown; requestId: string };
  expect(body).toMatchObject({ method: "chat_model", params: { conversationId: "c1", key: "standard/builder", choice } });
  expect(body.requestId).toBeTruthy();
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("reads conversation-scoped Beads through the authenticated read RPC", async () => {
  const issues = [{ id: "project-epic", title: "Sincronização Sienge", status: "in_progress", issueType: "epic", parentId: null }];
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response({ issues }));
  const client = new RemoteClient(fetcher);
  expect(await client.beads("c1")).toEqual({ issues });
  expect(fetcher).toHaveBeenCalledWith("/api/rpc", expect.objectContaining({ body: JSON.stringify({ method: "beads", params: { conversationId: "c1" } }), credentials: "same-origin" }));
});

it("sends queue operations once with the shared message identity", async () => {
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => response({ delivered: true }));
  const client = new RemoteClient(fetcher);
  for (const method of ["queue_edit", "queue_delete", "queue_send_now"] as const) {
    await client.mutate(method, { conversationId: "c1", messageId: "q1", ...(method === "queue_edit" ? { content: "Revisar PostgreSQL" } : {}) });
  }
  const bodies = fetcher.mock.calls.map(call => JSON.parse(String(call[1]?.body)) as { method: string; params: { messageId: string }; requestId: string });
  expect(bodies.map(body => body.method)).toEqual(["queue_edit", "queue_delete", "queue_send_now"]);
  expect(bodies.every(body => body.params.messageId === "q1")).toBe(true);
  expect(new Set(bodies.map(body => body.requestId)).size).toBe(3);
});

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const response = (data: unknown) => new Response(JSON.stringify({ ok: true, data }), { headers: { "Content-Type": "application/json" } });

it("accepts sanitized OpenCode Go quotas including a monthly anniversary reset", async () => {
  const usage = [{ alias: "opencode-go-pessoal", providerKind: "opencode-go", fetchedAt: 1, email: null, plan: "go", error: null, resetCredits: null, windows: [
    { id: "monthly", label: "Mensal", group: "OpenCode Go", thirdParty: false, durationSeconds: null, remainingPercent: 67, resetsAt: 1_800_000_000_000 },
  ] }];
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response(usage));
  expect(await new RemoteClient(fetcher).usage()).toEqual(usage);
});

it("consumes the QR fragment without leaving the pairing credential in the URL", () => {
  const history = { replaceState: vi.fn() };
  expect(takePairingToken({ hash: "#pair=one-time-secret&view=chat", pathname: "/", search: "?mobile=1" }, history)).toBe("one-time-secret");
  expect(history.replaceState).toHaveBeenCalledWith(null, "", "/?mobile=1#view=chat");
  expect(takePairingToken({ hash: "#view=chat", pathname: "/", search: "" }, history)).toBeNull();
  expect(history.replaceState).toHaveBeenCalledTimes(1);
});

it("pairs and restores metadata using only the same-origin cookie", async () => {
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => response({ deviceId: "phone-1", name: "Celular" }));
  const client = new RemoteClient(fetcher);
  expect(await client.pair("secret", "Celular")).toEqual({ deviceId: "phone-1", name: "Celular" });
  expect(fetcher).toHaveBeenCalledWith("/api/pair", expect.objectContaining({ method: "POST", credentials: "same-origin", body: JSON.stringify({ token: "secret", name: "Celular" }) }));
  expect(await client.session()).toEqual({ deviceId: "phone-1", name: "Celular" });
  expect(fetcher).toHaveBeenLastCalledWith("/api/session", expect.objectContaining({ method: "GET", cache: "no-store", credentials: "same-origin" }));
  expect(fetcher.mock.calls[1][1]?.headers).not.toHaveProperty("Authorization");
});

it("preserves expired/revoked error codes so the UI requests a new QR", async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ ok: false, error: { code: "session_expired", message: "Acesso revogado" } }), { status: 401 }));
  const client = new RemoteClient(fetcher);
  await expect(client.session()).rejects.toMatchObject({ code: "session_expired", expired: true, message: "Acesso revogado" });
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("rejects malformed successful responses instead of trusting an envelope", async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: [] })));
  await expect(new RemoteClient(fetcher).session()).rejects.toBeInstanceOf(RemoteError);
});

it("adds a unique mutation ID on LAN HTTP when randomUUID is unavailable", async () => {
  vi.stubGlobal("crypto", { getRandomValues: crypto.getRandomValues.bind(crypto) });
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => response({ ok: true }));
  const client = new RemoteClient(fetcher);
  await client.mutate("cancel", { conversationId: "c1", turnId: "t1" });
  await client.mutate("cancel", { conversationId: "c1", turnId: "t2" });
  const bodies = fetcher.mock.calls.map(call => JSON.parse(String(call[1]?.body)) as { method: string; params: unknown; requestId: string });
  expect(bodies[0].requestId).toMatch(/^[a-f0-9]{32}$/);
  expect(bodies[1].requestId).not.toBe(bodies[0].requestId);
  expect(bodies[0]).toMatchObject({ method: "cancel", params: { conversationId: "c1", turnId: "t1" } });
});

it("aborts a request at its deadline and never retries an uncertain mutation", async () => {
  vi.useFakeTimers();
  const fetcher = vi.fn<typeof fetch>().mockImplementation((_url, init) => new Promise((_resolve, reject) => { init?.signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError"))); }));
  const client = new RemoteClient(fetcher, 50);
  const result = expect(client.mutate("message", { conversationId: "c1", content: "Olá" })).rejects.toMatchObject({ code: "timeout" });
  await vi.advanceTimersByTimeAsync(60);
  await result;
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(fetcher.mock.calls[0][1]?.signal?.aborted).toBe(true);
});
