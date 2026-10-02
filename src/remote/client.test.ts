import { afterEach, expect, it, vi } from "vitest";
import { RemoteClient, RemoteError, takePairingToken } from "./client";

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const response = (data: unknown) => new Response(JSON.stringify({ ok: true, data }), { headers: { "Content-Type": "application/json" } });

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
