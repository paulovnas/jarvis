import { afterEach, beforeEach, expect, it, vi } from "vitest";

const adapters = vi.hoisted(() => ({
  chromium: vi.fn(), firefox: vi.fn(), restore: vi.fn(), releaseAll: vi.fn(),
  execute: vi.fn(), removed: vi.fn(), updated: vi.fn(),
}));
vi.mock("./browser", () => ({ BrowserController: class {
  constructor(readonly epoch: string) { adapters.chromium(epoch); }
  restore = adapters.restore;
  releaseAll = adapters.releaseAll;
  execute = adapters.execute;
  removed = adapters.removed;
  updated = adapters.updated;
} }));
vi.mock("./firefox", () => ({ FirefoxBrowserController: class {
  constructor(readonly epoch: string) { adapters.firefox(epoch); }
  restore = adapters.restore;
  releaseAll = adapters.releaseAll;
  execute = adapters.execute;
  removed = adapters.removed;
  updated = adapters.updated;
} }));

class Socket {
  static OPEN = 1;
  static CLOSING = 2;
  static instances: Socket[] = [];
  readyState = 0;
  onopen?: () => void;
  onmessage?: (event: { data: string }) => void;
  onerror?: () => void;
  onclose?: () => void;
  send = vi.fn();
  close = vi.fn(() => { this.readyState = 3; this.onclose?.(); });
  constructor(readonly url: string) { Socket.instances.push(this); }
  receive(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) }); }
  ready() { this.readyState = 1; this.onopen?.(); this.receive({ type: "ready", version: 1 }); }
}

const pairing = { version: 1, endpoint: "ws://127.0.0.1:17373/extension", token: "a".repeat(64) };
type MessageListener = (value: unknown, sender: { id: string; url?: string; frameId?: number; tab?: { id: number } }, respond: (value: unknown) => void) => boolean;
function api(firefox: boolean) {
  return {
    runtime: { id: "jarvis", getURL: (path: string) => `${firefox ? "moz" : "chrome"}-extension://id/${path}`, getManifest: () => ({ version: "1.1.0" }), openOptionsPage: vi.fn(), onMessage: { addListener: vi.fn<(listener: MessageListener) => void>() } },
    storage: {
      local: { get: vi.fn().mockResolvedValue({ pairing, instanceId: "profile" }), set: vi.fn(), remove: vi.fn(), ...(firefox ? {} : { setAccessLevel: vi.fn() }) },
      session: { get: vi.fn().mockResolvedValue({ epoch: "session" }), set: vi.fn(), ...(firefox ? {} : { setAccessLevel: vi.fn() }) },
    },
    action: { setBadgeText: vi.fn(), setBadgeBackgroundColor: vi.fn(), onClicked: { addListener: vi.fn() } },
    alarms: { create: vi.fn(), onAlarm: { addListener: vi.fn() } },
    tabs: { onRemoved: { addListener: vi.fn() }, onUpdated: { addListener: vi.fn() } },
    ...(firefox ? {} : { debugger: { onDetach: { addListener: vi.fn() }, onEvent: { addListener: vi.fn() } } }),
  };
}

beforeEach(() => {
  vi.resetModules(); vi.clearAllMocks(); vi.useFakeTimers();
  Socket.instances = [];
  adapters.restore.mockResolvedValue(undefined);
  adapters.releaseAll.mockResolvedValue(undefined);
  adapters.execute.mockResolvedValue({ tabs: [] });
  vi.stubGlobal("WebSocket", Socket);
});
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.unstubAllGlobals(); });
async function flush() { for (let count = 0; count < 12; count++) await Promise.resolve(); }

it.each([true, false])("starts the correct adapter and pairs without accessing absent APIs (Firefox=%s)", async firefox => {
  const browser = api(firefox);
  vi.stubGlobal("browser", firefox ? browser : undefined);
  vi.stubGlobal("chrome", firefox ? undefined : browser);
  await import("./worker"); await flush();
  expect(firefox ? adapters.firefox : adapters.chromium).toHaveBeenCalledWith("session");
  expect(firefox ? adapters.chromium : adapters.firefox).not.toHaveBeenCalled();
  expect(adapters.restore).toHaveBeenCalledOnce();
  expect(Socket.instances).toHaveLength(1);
  const socket = Socket.instances[0]; socket.ready();
  expect(JSON.parse(socket.send.mock.calls[0][0] as string)).toMatchObject({ type: "hello", token: pairing.token, epoch: "session", instanceId: "profile", label: firefox ? "Firefox" : "Navegador Chromium" });
  expect(browser.action.setBadgeText).toHaveBeenLastCalledWith({ text: "ON" });
});

it("never replays a received action after reconnecting", async () => {
  const browser = api(true); vi.stubGlobal("browser", browser);
  await import("./worker"); await flush();
  const socket = Socket.instances[0]; socket.ready();
  const request = { type: "request", id: "once", conversationId: "chat", request: { action: "list" } };
  socket.receive(request); await flush();
  expect(adapters.execute).toHaveBeenCalledOnce();
  socket.close(); await vi.advanceTimersByTimeAsync(1000);
  const next = Socket.instances[1]; next.ready(); next.receive(request); await flush();
  expect(adapters.execute).toHaveBeenCalledOnce();
  expect(next.send.mock.calls.map(([value]) => JSON.parse(value as string))).toContainEqual(expect.objectContaining({ id: "once", ok: false, error: expect.objectContaining({ code: "browser_outcome_unknown" }) }));
});

it.each([true, false])("accepts pairing and status from its own options tab (Firefox=%s)", async firefox => {
  const browser = api(firefox);
  vi.stubGlobal("browser", firefox ? browser : undefined);
  vi.stubGlobal("chrome", firefox ? undefined : browser);
  await import("./worker"); await flush();
  const listener = browser.runtime.onMessage.addListener.mock.calls[0][0];
  // Firefox includes the containing tab for options pages, as well as content
  // scripts. The browser-supplied extension URL identifies the trusted page.
  const sender = { id: "jarvis", url: browser.runtime.getURL("options.html"), tab: { id: 1 }, frameId: 0 };
  const respond = vi.fn();
  expect(listener({ type: "status" }, sender, respond)).toBe(true);
  await flush();
  expect(respond).toHaveBeenLastCalledWith(expect.objectContaining({ ok: true, status: expect.objectContaining({ state: "connecting" }) }));
  expect(listener({ type: "connect", pairing }, sender, respond)).toBe(true);
  await flush();
  expect(browser.storage.local.set).toHaveBeenCalledWith({ pairing });
  expect(adapters.releaseAll).toHaveBeenCalledOnce();
  expect(Socket.instances).toHaveLength(2);
  expect(respond).toHaveBeenLastCalledWith(expect.objectContaining({ ok: true }));
});

it("allows its own options to disconnect but rejects untrusted and embedded senders", async () => {
  const browser = api(true); vi.stubGlobal("browser", browser);
  await import("./worker"); await flush();
  const listener = browser.runtime.onMessage.addListener.mock.calls[0][0];
  const respond = vi.fn();
  const options = browser.runtime.getURL("options.html");
  for (const sender of [
    { id: "jarvis", url: "https://example.test/options.html", tab: { id: 1 }, frameId: 0 },
    { id: "other", url: options },
    { id: "jarvis", url: browser.runtime.getURL("other.html") },
    { id: "jarvis", url: `${options}.untrusted` },
    { id: "jarvis", url: options, tab: { id: 1 }, frameId: 1 },
    { id: "jarvis" },
  ]) expect(listener({ type: "disconnect" }, sender, respond)).toBe(false);
  expect(adapters.releaseAll).not.toHaveBeenCalled();
  expect(respond).not.toHaveBeenCalled();
  expect(listener({ type: "disconnect" }, { id: "jarvis", url: options }, respond)).toBe(true);
  await flush();
  expect(browser.storage.local.remove).toHaveBeenCalledWith("pairing");
  expect(adapters.releaseAll).toHaveBeenCalledOnce();
  expect(respond).toHaveBeenCalledWith(expect.objectContaining({ ok: true, status: expect.objectContaining({ state: "disconnected" }) }));
  await vi.advanceTimersByTimeAsync(20_000);
  expect(Socket.instances).toHaveLength(1);
});
