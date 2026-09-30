import { beforeEach, describe, expect, it, vi } from "vitest";
import { BrowserController } from "./browser";
import { address, BrowserError, CommandQueue, nativeTabId, pairingSchema, requestSchema, tabHandle, validateMethod, type Request } from "./protocol";

const epoch = "session-1";
const chromeMock = {
  storage: { session: { get: vi.fn(), set: vi.fn().mockResolvedValue(undefined) } },
  tabs: {
    query: vi.fn(), get: vi.fn(), create: vi.fn(), update: vi.fn(), remove: vi.fn().mockResolvedValue(undefined),
    reload: vi.fn(), goBack: vi.fn(), goForward: vi.fn(),
  },
  windows: { update: vi.fn(), create: vi.fn() },
  debugger: { getTargets: vi.fn(), attach: vi.fn(), detach: vi.fn(), sendCommand: vi.fn() },
};
const tab = { id: 10, windowId: 1, url: "https://example.org", title: "Example", status: "complete" };
const request = (action: Request["request"]["action"], values: Partial<Request["request"]> = {}, conversationId = "chat-a"): Request => ({ type: "request", id: crypto.randomUUID(), conversationId, request: { action, ...values } });

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("chrome", chromeMock);
  chromeMock.storage.session.get.mockResolvedValue({});
  chromeMock.tabs.query.mockResolvedValue([tab]);
  chromeMock.tabs.get.mockResolvedValue(tab);
  chromeMock.tabs.create.mockResolvedValue({ ...tab, id: 20 });
  chromeMock.tabs.update.mockResolvedValue(tab);
  chromeMock.windows.update.mockResolvedValue({});
  chromeMock.debugger.getTargets.mockResolvedValue([]);
  chromeMock.debugger.attach.mockResolvedValue(undefined);
  chromeMock.debugger.detach.mockResolvedValue(undefined);
  chromeMock.debugger.sendCommand.mockResolvedValue({});
});

describe("extension trust boundary", () => {
  it("accepts only local authenticated pairing codes and page-scoped operations", () => {
    const config = { version: 1, endpoint: "ws://127.0.0.1:17373/extension", token: "a".repeat(64) };
    expect(pairingSchema.parse(config)).toEqual(config);
    for (const endpoint of ["ws://evil.example/extension", "ws://localhost:17373/extension", "ws://127.0.0.1:17373/extension?token=abc", "http://127.0.0.1:17373/extension"]) expect(pairingSchema.safeParse({ ...config, endpoint }).success).toBe(false);
    for (const method of ["Browser.close", "Target.createTarget", "Network.getAllCookies", "Page.setDownloadBehavior", "DOM.setFileInputFiles"]) expect(() => validateMethod(method, {})).toThrow(BrowserError);
    expect(() => validateMethod("Page.navigate", { url: "file:///etc/passwd" })).toThrow(BrowserError);
    expect(() => validateMethod("DOM.getDocument", {})).not.toThrow();
    expect(() => validateMethod("Runtime.evaluate", { expression: "document.title" })).not.toThrow();
  });

  it("rejects stale session handles, protected pages and oversized arguments", () => {
    expect(nativeTabId(epoch, tabHandle(epoch, 10))).toBe(10);
    expect(() => nativeTabId(epoch, "ext:old-session:10")).toThrow("outra sessão");
    for (const url of ["chrome://settings", "https://user:pass@example.org", "https://chromewebstore.google.com/detail/example"]) expect(() => address(url)).toThrow();
    expect(requestSchema.safeParse(request("fill", { text: "x".repeat(8001) })).success).toBe(false);
    expect(requestSchema.safeParse(request("scroll", { y: Infinity })).success).toBe(false);
  });
});

describe("owned browser tabs", () => {
  it("creates a new tab instead of commandeering the user's focused page", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [], activeId: null });
    await browser.execute(request("open", { url: "https://example.org/work" }));
    expect(chromeMock.tabs.create).toHaveBeenCalledWith({ url: "https://example.org/work", active: true });
    expect(chromeMock.tabs.update).not.toHaveBeenCalled();
    expect(chromeMock.debugger.attach).toHaveBeenCalledWith({ tabId: 20 }, "1.3");
  });

  it("discovery does not attach and ownership prevents another chat operating a tab", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await expect(browser.execute(request("discover"))).resolves.toMatchObject({ tabs: [{ id: tabHandle(epoch, 10), owned: false }] });
    expect(chromeMock.debugger.attach).not.toHaveBeenCalled();
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    await expect(browser.execute(request("navigate", { id: tabHandle(epoch, 10), url: "https://example.org" }, "chat-b"))).rejects.toMatchObject({ code: "browser_tab_not_owned" });
    await expect(browser.execute(request("attach", { id: tabHandle(epoch, 10) }, "chat-b"))).rejects.toMatchObject({ code: "browser_tab_busy" });
  });

  it("prunes debugger ownership without closing or navigating a personal tab", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    await browser.execute(request("prune", { retained: [] }, ""));
    expect(chromeMock.debugger.detach).toHaveBeenCalledWith({ tabId: 10 });
    expect(chromeMock.tabs.remove).not.toHaveBeenCalled();
    expect(chromeMock.tabs.update).not.toHaveBeenCalled();
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [] });
  });

  it("restores ownership after worker suspension without replaying actions", async () => {
    chromeMock.storage.session.get.mockResolvedValue({ owners: { 10: { conversationId: "chat-a", created: false } }, active: { "chat-a": 10 } });
    chromeMock.debugger.getTargets.mockResolvedValue([{ tabId: 10, attached: true }]);
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [{ id: tabHandle(epoch, 10) }], activeId: tabHandle(epoch, 10) });
    expect(chromeMock.tabs.create).not.toHaveBeenCalled();
    expect(chromeMock.tabs.update).not.toHaveBeenCalled();
    expect(chromeMock.debugger.sendCommand).toHaveBeenCalledExactlyOnceWith({ tabId: 10 }, "Runtime.enable", {});
  });

  it("reconnects a created blank tab without converting it into a personal tab", async () => {
    chromeMock.tabs.create.mockResolvedValue({ ...tab, id: 20, url: "about:blank" });
    chromeMock.tabs.get.mockResolvedValue({ ...tab, id: 20, url: "about:blank" });
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await browser.execute(request("open"));
    browser.onDetach(20);
    await expect(browser.execute(request("attach", { id: tabHandle(epoch, 20) }))).resolves.toMatchObject({ tabs: [{ url: "about:blank" }] });
    await expect(browser.execute(request("console", { id: tabHandle(epoch, 20) }))).resolves.toEqual({ logs: [] });
    expect(chromeMock.storage.session.set).toHaveBeenLastCalledWith(expect.objectContaining({ owners: { 20: { conversationId: "chat-a", created: true } } }));
  });

  it("fences a pending tab creation during explicit disconnect without adopting or closing it", async () => {
    let complete!: (value: unknown) => void;
    chromeMock.tabs.create.mockImplementation(() => new Promise(resolve => { complete = resolve; }));
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    const opening = browser.execute(request("open", { url: "https://example.org" }));
    await vi.waitFor(() => expect(complete).toBeDefined());
    const disconnecting = browser.releaseAll();
    complete({ ...tab, id: 20 });
    await expect(opening).rejects.toMatchObject({ code: "browser_outcome_unknown" });
    await disconnecting;
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [] });
    expect(chromeMock.debugger.attach).not.toHaveBeenCalled();
    expect(chromeMock.tabs.remove).not.toHaveBeenCalled();
  });

  it("does not mistake DevTools for its own debugger after worker suspension", async () => {
    chromeMock.storage.session.get.mockResolvedValue({ owners: { 10: { conversationId: "chat-a", created: false } } });
    chromeMock.debugger.getTargets.mockResolvedValue([{ tabId: 10, attached: true }]);
    chromeMock.debugger.sendCommand.mockRejectedValue(new Error("Debugger is not attached to the tab with id: 10"));
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await expect(browser.execute(request("snapshot", { id: tabHandle(epoch, 10) }))).rejects.toMatchObject({ code: "browser_debugger_detached" });
    chromeMock.debugger.sendCommand.mockResolvedValue({});
    await expect(browser.execute(request("attach", { id: tabHandle(epoch, 10) }))).resolves.toMatchObject({ backend: "extension" });
    expect(chromeMock.debugger.attach).toHaveBeenCalledExactlyOnceWith({ tabId: 10 }, "1.3");
  });

  it("returns a recoverable error after DevTools detaches and only reconnects explicitly", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    browser.onDetach(10);
    await expect(browser.execute(request("snapshot", { id: tabHandle(epoch, 10) }))).rejects.toMatchObject({ code: "browser_debugger_detached" });
    expect(chromeMock.debugger.attach).toHaveBeenCalledTimes(1);
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    expect(chromeMock.debugger.attach).toHaveBeenCalledTimes(2);
  });

  it("allows reconnecting an owned tab when the conversation is at its tab limit", async () => {
    const owners = Object.fromEntries(Array.from({ length: 12 }, (_, i) => [i + 1, { conversationId: "chat-a", created: false }]));
    chromeMock.storage.session.get.mockResolvedValue({ owners });
    chromeMock.tabs.query.mockResolvedValue(Array.from({ length: 12 }, (_, i) => ({ ...tab, id: i + 1 })));
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    browser.onDetach(10);
    await expect(browser.execute(request("attach", { id: tabHandle(epoch, 10) }))).resolves.toMatchObject({ backend: "extension" });
    await expect(browser.execute(request("open", { url: "https://example.org" }))).rejects.toMatchObject({ code: "browser_tab_limit" });
  });

  it("discards queued operations after a connection ends without sending a click", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    let connected = true;
    let finish!: (value: unknown) => void;
    chromeMock.debugger.sendCommand.mockImplementation((_source, method: string) => method === "Runtime.evaluate" ? new Promise(resolve => { finish = resolve; }) : Promise.resolve({}));
    const running = browser.execute(request("evaluate", { id: tabHandle(epoch, 10), expression: "document.title" }), () => connected);
    await vi.waitFor(() => expect(finish).toBeDefined());
    const queued = browser.execute(request("click", { id: tabHandle(epoch, 10), element: "stale-1" }), () => connected);
    connected = false;
    finish({ result: { value: "Already evaluated" } });
    await expect(running).resolves.toBe("Already evaluated");
    await expect(queued).rejects.toMatchObject({ code: "browser_outcome_unknown" });
    expect(chromeMock.debugger.sendCommand.mock.calls.some(call => call[1] === "Input.dispatchMouseEvent")).toBe(false);
  });

  it("cancels a queued request while retaining the tab and connection for subsequent work", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    let finish!: (value: unknown) => void;
    let cancelled = false;
    chromeMock.debugger.sendCommand.mockImplementation((_source, method: string) => method === "Runtime.evaluate" ? new Promise(resolve => { finish = resolve; }) : Promise.resolve({}));
    const running = browser.execute(request("evaluate", { id: tabHandle(epoch, 10), expression: "document.title" }));
    await vi.waitFor(() => expect(finish).toBeDefined());
    const queued = browser.execute(request("close", { id: tabHandle(epoch, 10) }), () => !cancelled);
    cancelled = true;
    finish({ result: { value: "Example" } });
    await running;
    await expect(queued).rejects.toMatchObject({ code: "browser_outcome_unknown" });
    await expect(browser.execute(request("console", { id: tabHandle(epoch, 10) }))).resolves.toEqual({ logs: [] });
    expect(chromeMock.tabs.remove).not.toHaveBeenCalled();
  });

  it("focuses the requested element before pressing a key and clears empty fills", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    chromeMock.debugger.sendCommand.mockImplementation((_source, method: string) => Promise.resolve(method === "Page.getFrameTree" ? { frameTree: { frame: { id: "main" } } } : method === "Page.createIsolatedWorld" ? { executionContextId: 7 } : { result: { value: { ready: true, token: "prepared-field", x: 10, y: 10, tag: "input", elements: [{ id: "field-1" }] } } }));
    await browser.execute(request("snapshot", { id: tabHandle(epoch, 10) }));
    chromeMock.debugger.sendCommand.mockClear();
    await browser.execute(request("press", { id: tabHandle(epoch, 10), element: "field-1", key: "Enter" }));
    const evaluation = chromeMock.debugger.sendCommand.mock.calls.find(call => call[1] === "Runtime.evaluate");
    expect(evaluation?.[2]).toMatchObject({ contextId: 7, expression: expect.stringContaining('"mode":"press","element":"field-1"') });
    chromeMock.debugger.sendCommand.mockClear();
    await browser.execute(request("fill", { id: tabHandle(epoch, 10), element: "field-1", text: "" }));
    expect(chromeMock.debugger.sendCommand).toHaveBeenCalledWith({ tabId: 10 }, "Input.dispatchKeyEvent", expect.objectContaining({ key: "Backspace", type: "keyDown" }));
    expect(chromeMock.debugger.sendCommand.mock.calls.some(call => call[1] === "Input.insertText")).toBe(false);
    browser.event(10, "Page.navigatedWithinDocument", { url: "https://example.org/#changed" });
    await expect(browser.execute(request("press", { id: tabHandle(epoch, 10), element: "field-1", key: "Enter" }))).rejects.toMatchObject({ code: "browser_stale_element" });
  });

  it("bounds diagnostics and returns filtered paginated summaries without response bodies", async () => {
    const browser = new BrowserController(epoch, vi.fn());
    await browser.restore();
    await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
    for (let i = 0; i < 210; i++) browser.event(10, "Network.requestWillBeSent", { requestId: String(i), request: { method: "GET", url: `https://example.org/${i}` } });
    browser.event(10, "Network.responseReceived", { requestId: "209", response: { status: 500, body: "not included" } });
    await expect(browser.execute(request("network", { id: tabHandle(epoch, 10), limit: 2 }))).resolves.toMatchObject({ total: 200, requests: [{ id: "10" }, { id: "11" }] });
    await expect(browser.execute(request("network", { id: tabHandle(epoch, 10), filter: "500" }))).resolves.toEqual({ total: 1, requests: [{ id: "209", method: "GET", url: "https://example.org/209", type: "", status: 500 }], offset: 0, limit: 30 });
    await expect(browser.execute(request("response_body", { id: tabHandle(epoch, 10), requestId: "0" }))).rejects.toMatchObject({ code: "browser_request_expired" });
  });
});

it("serializes operations on a tab while allowing other tabs to proceed", async () => {
  const queue = new CommandQueue(), calls: string[] = [];
  let finish!: () => void;
  const first = queue.run(1, async () => { calls.push("first"); await new Promise<void>(resolve => { finish = resolve; }); });
  const second = queue.run(1, async () => { calls.push("second"); });
  await queue.run(2, async () => { calls.push("other"); });
  expect(calls).toEqual(["first", "other"]);
  finish();
  await Promise.all([first, second]);
  expect(calls).toEqual(["first", "other", "second"]);
});

type PageArgs = { action: string; mode?: string; element?: string; locator?: { role?: string; name?: string }; offset?: number; limit?: number };
function pageArgs(raw: unknown): PageArgs {
  const expression = String((raw as { expression?: string }).expression ?? "");
  return JSON.parse(expression.slice(expression.lastIndexOf("})(") + 3, -1)) as PageArgs;
}
async function interactiveBrowser(handler: (args: PageArgs) => Record<string, unknown> = () => ({ ready: true, x: 10, y: 20, tag: "button", token: "prepared-target" })) {
  const browser = new BrowserController(epoch, vi.fn());
  await browser.restore();
  await browser.execute(request("attach", { id: tabHandle(epoch, 10) }));
  chromeMock.debugger.sendCommand.mockImplementation((_source, method: string, params: Record<string, unknown>) => {
    if (method === "Page.getFrameTree") return Promise.resolve({ frameTree: { frame: { id: "main", url: tab.url }, childFrames: [{ frame: { id: "child", parentId: "main", url: "https://embedded.example" } }] } });
    if (method === "Page.createIsolatedWorld") return Promise.resolve({ executionContextId: params.frameId === "main" ? 7 : 8 });
    if (method === "Runtime.evaluate") {
      const args = pageArgs(params);
      return Promise.resolve({ result: { value: args.action === "snapshot" ? { elements: [{ id: "target-1", name: "Save", role: "button" }], total: 1, offset: args.offset ?? 0, limit: args.limit ?? 100 }
        : args.action === "finish" ? { blocked: false } : handler(args) } });
    }
    if (method === "DOM.getFrameOwner") return Promise.resolve({ backendNodeId: 1 });
    if (method === "DOM.resolveNode") return Promise.resolve({ object: { objectId: "frame-owner" } });
    if (method === "Runtime.callFunctionOn") {
      const args = (params.arguments as { value: { action: string; x?: number; y?: number } }[])[0].value;
      return Promise.resolve({ result: { value: args.action === "point" ? { ready: true, x: (args.x ?? 0) + 100, y: (args.y ?? 0) + 200 } : { ready: true, blocked: false } } });
    }
    return Promise.resolve({});
  });
  return browser;
}

describe("reliable browser interactions", () => {
  it("validates unique semantic targets and bounded wait contracts before dispatch", () => {
    expect(requestSchema.safeParse(request("click", { locator: { role: "button", name: "Save" } })).success).toBe(true);
    for (const locator of [{}, { name: "Save" }, { label: "Save", text: "Save" }]) expect(requestSchema.safeParse(request("click", { locator })).success).toBe(false);
    expect(requestSchema.safeParse(request("click", { element: "one", locator: { text: "Save" } })).success).toBe(false);
    expect(requestSchema.safeParse(request("wait", { state: "ready", timeoutMs: 15001 })).success).toBe(false);
    expect(requestSchema.safeParse(request("wait", { state: "hidden" })).success).toBe(false);
    expect(requestSchema.safeParse(request("wait", { state: "ready" })).success).toBe(true);
  });

  it("waits for actionability and dispatches a semantic click exactly once", async () => {
    let attempts = 0;
    const browser = await interactiveBrowser(args => args.action === "prepare" && attempts++ === 0
      ? { ready: false, code: "browser_element_disabled", reason: "Button is disabled" }
      : { ready: true, x: 10, y: 20, token: "prepared-target" });
    await expect(browser.execute(request("click", { id: tabHandle(epoch, 10), locator: { role: "button", name: "Save" } }))).resolves.toMatchObject({ dispatched: true, frameId: "main" });
    expect(attempts).toBe(2);
    expect(chromeMock.debugger.sendCommand.mock.calls.filter(call => call[1] === "Input.dispatchMouseEvent")).toHaveLength(2);
  });

  it("returns preflight diagnostics and sends no input for a covered or ambiguous target", async () => {
    for (const code of ["browser_element_obscured", "browser_ambiguous_element"]) {
      const browser = await interactiveBrowser(() => ({ ready: false, code, reason: "Target is unavailable" }));
      await expect(browser.execute(request("click", { id: tabHandle(epoch, 10), locator: { text: "Save" }, timeoutMs: 0 }))).rejects.toMatchObject({ details: { dispatched: false, phase: "prepare" } });
      expect(chromeMock.debugger.sendCommand.mock.calls.some(call => String(call[1]).startsWith("Input."))).toBe(false);
      chromeMock.debugger.sendCommand.mockClear();
    }
  });

  it("cancels an in-flight preflight without input while preserving the tab", async () => {
    let connected = true;
    const browser = await interactiveBrowser(args => {
      if (args.action === "prepare") connected = false;
      return { ready: true, x: 10, y: 20, token: "prepared-target" };
    });
    await expect(browser.execute(request("click", { id: tabHandle(epoch, 10), locator: { text: "Save" } }), () => connected)).rejects.toMatchObject({ code: "browser_outcome_unknown" });
    expect(chromeMock.debugger.sendCommand.mock.calls.some(call => String(call[1]).startsWith("Input."))).toBe(false);
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [{ id: tabHandle(epoch, 10) }] });
  });

  it("never retries after a partial click loses its confirmation", async () => {
    const browser = await interactiveBrowser();
    const implementation = chromeMock.debugger.sendCommand.getMockImplementation()!;
    chromeMock.debugger.sendCommand.mockImplementation((source, method: string, params: Record<string, unknown>) => {
      if (method === "Input.dispatchMouseEvent" && params.type === "mouseReleased") return Promise.reject(new Error("Connection lost after mousedown"));
      return implementation(source, method, params);
    });
    await expect(browser.execute(request("click", { id: tabHandle(epoch, 10), locator: { text: "Save" } }))).rejects.toMatchObject({ code: "browser_outcome_unknown", details: { phase: "dispatch", dispatched: true } });
    expect(chromeMock.debugger.sendCommand.mock.calls.filter(call => call[1] === "Input.dispatchMouseEvent")).toHaveLength(2);
  });

  it("keeps snapshots scoped and paginated while exposing frames for selection", async () => {
    const browser = await interactiveBrowser();
    await expect(browser.execute(request("snapshot", { id: tabHandle(epoch, 10), frameId: "child", offset: 10, limit: 20 }))).resolves.toMatchObject({ frameId: "child", offset: 10, limit: 20, frames: [{ id: "main" }, { id: "child", parentId: "main" }] });
    expect(chromeMock.debugger.sendCommand).toHaveBeenCalledWith({ tabId: 10 }, "Page.createIsolatedWorld", expect.objectContaining({ frameId: "child" }));
    await browser.execute(request("click", { id: tabHandle(epoch, 10), element: "target-1" }));
    expect(chromeMock.debugger.sendCommand).toHaveBeenCalledWith({ tabId: 10 }, "Input.dispatchMouseEvent", expect.objectContaining({ x: 110, y: 220, type: "mousePressed" }));
    await expect(browser.execute(request("click", { id: tabHandle(epoch, 10), element: "target-1", frameId: "main" }))).rejects.toMatchObject({ code: "browser_invalid_request" });
  });

  it("routes cross-origin iframe worlds and response bodies through their debugger session", async () => {
    const browser = await interactiveBrowser();
    browser.event(10, "Target.attachedToTarget", { sessionId: "remote-session", targetInfo: { targetId: "remote", type: "iframe", url: "https://remote.example" } });
    const implementation = chromeMock.debugger.sendCommand.getMockImplementation()!;
    chromeMock.debugger.sendCommand.mockImplementation((source: { sessionId?: string }, method: string, params: Record<string, unknown>) => {
      if (source.sessionId === "remote-session" && method === "Page.getFrameTree") return Promise.resolve({ frameTree: { frame: { id: "remote", parentId: "main", url: "https://remote.example" } } });
      return implementation(source, method, params);
    });
    await expect(browser.execute(request("snapshot", { id: tabHandle(epoch, 10), frameId: "remote" }))).resolves.toMatchObject({ frameId: "remote" });
    expect(chromeMock.debugger.sendCommand).toHaveBeenCalledWith({ tabId: 10, sessionId: "remote-session" }, "Page.createIsolatedWorld", expect.objectContaining({ frameId: "remote" }));
    browser.event(10, "Network.requestWillBeSent", { requestId: "5", request: { method: "GET", url: "https://remote.example" } }, "remote-session");
    await browser.execute(request("response_body", { id: tabHandle(epoch, 10), requestId: "remote-session:5" }));
    expect(chromeMock.debugger.sendCommand).toHaveBeenCalledWith({ tabId: 10, sessionId: "remote-session" }, "Network.getResponseBody", { requestId: "5" });
  });

  it("waits for document readiness without dispatching an interaction", async () => {
    const browser = await interactiveBrowser(() => ({ ready: true }));
    await expect(browser.execute(request("wait", { id: tabHandle(epoch, 10), state: "ready" }))).resolves.toMatchObject({ ready: true, state: "ready" });
    expect(chromeMock.debugger.sendCommand.mock.calls.some(call => String(call[1]).startsWith("Input."))).toBe(false);
  });

  it("sends no input when frame guard setup has already consumed its protection window", async () => {
    const browser = await interactiveBrowser();
    let now = Date.now();
    const clock = vi.spyOn(Date, "now").mockImplementation(() => now);
    const implementation = chromeMock.debugger.sendCommand.getMockImplementation()!;
    chromeMock.debugger.sendCommand.mockImplementation((source, method: string, params: Record<string, unknown>) => {
      if (method === "Runtime.callFunctionOn" && (params.arguments as { value: { action: string } }[])[0].value.action === "guard") now += 3100;
      return implementation(source, method, params);
    });
    try {
      await expect(browser.execute(request("click", { id: tabHandle(epoch, 10), frameId: "child", locator: { text: "Save" } }))).rejects.toMatchObject({ code: "browser_guard_unavailable", details: { dispatched: false } });
      expect(chromeMock.debugger.sendCommand.mock.calls.some(call => String(call[1]).startsWith("Input."))).toBe(false);
    } finally { clock.mockRestore(); }
  });

  it("does not insert text after a key handler changes focus during select-all", async () => {
    let verified = 0;
    const browser = await interactiveBrowser(args => args.action === "verify" && ++verified > 1
      ? { ready: false, code: "browser_focus_changed" }
      : { ready: true, x: 10, y: 20, tag: "input", token: "prepared-target" });
    await expect(browser.execute(request("fill", { id: tabHandle(epoch, 10), locator: { label: "Email" }, text: "new@example.org" }))).rejects.toMatchObject({ code: "browser_outcome_unknown", details: { dispatched: true } });
    expect(chromeMock.debugger.sendCommand.mock.calls.some(call => call[1] === "Input.insertText")).toBe(false);
  });
});
