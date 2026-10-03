import { beforeEach, describe, expect, it, vi } from "vitest";
import { FirefoxBrowserController, type FirefoxApi } from "./firefox";
import { firefoxConsole, firefoxInput } from "./firefox-page";
import { tabHandle, type Request } from "./protocol";

const epoch = "firefox-test";
const tab = { id: 10, windowId: 1, title: "Example", url: "https://example.org", status: "complete" };
const listener = () => ({ addListener: vi.fn<FirefoxApi["webRequest"]["onBeforeRequest"]["addListener"]>() });
const stream = () => ({ ondata: null, onstop: null, onerror: null, write: vi.fn(), close: vi.fn(), disconnect: vi.fn() });
const mocks = {
  storage: { session: { get: vi.fn(), set: vi.fn().mockResolvedValue(undefined) } },
  tabs: { query: vi.fn(), get: vi.fn(), create: vi.fn(), update: vi.fn(), remove: vi.fn(), reload: vi.fn(), goBack: vi.fn(), goForward: vi.fn(), captureTab: vi.fn() },
  windows: { create: vi.fn(), update: vi.fn() },
  webNavigation: { getAllFrames: vi.fn() },
  scripting: { executeScript: vi.fn<FirefoxApi["scripting"]["executeScript"]>() },
  webRequest: { onBeforeRequest: listener(), onHeadersReceived: listener(), onCompleted: listener(), onErrorOccurred: listener(), filterResponseData: vi.fn<FirefoxApi["webRequest"]["filterResponseData"]>() },
};
const request = (action: Request["request"]["action"], values: Partial<Request["request"]> = {}, conversationId = "chat-a"): Request => ({ type: "request", id: crypto.randomUUID(), conversationId, request: { action, ...values } });
const handle = tabHandle(epoch, 10);
// Vitest's Mock wrapper erases generic argument tuples from executeScript.
const controller = () => new FirefoxBrowserController(epoch, vi.fn(), mocks as unknown as FirefoxApi);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.storage.session.get.mockResolvedValue({}); mocks.tabs.query.mockResolvedValue([tab]); mocks.tabs.get.mockResolvedValue(tab);
  mocks.tabs.create.mockResolvedValue({ ...tab, id: 20 }); mocks.tabs.update.mockResolvedValue(tab); mocks.tabs.remove.mockResolvedValue(undefined);
  mocks.webNavigation.getAllFrames.mockResolvedValue([{ frameId: 0, parentFrameId: -1, url: tab.url }]);
  mocks.scripting.executeScript.mockResolvedValue([{ frameId: 0, result: {} }]); mocks.webRequest.filterResponseData.mockImplementation(stream);
  document.body.innerHTML = "";
});

describe("Firefox owned tabs", () => {
  it("discovers only ordinary web pages without accessing their DOM", async () => {
    mocks.tabs.query.mockResolvedValue([tab, { ...tab, id: 11, url: "about:preferences" }, { ...tab, id: 12, url: "https://addons.mozilla.org/en-US/firefox/" }]);
    const browser = controller(); await browser.restore();
    await expect(browser.execute(request("discover"))).resolves.toEqual({ tabs: [{ id: handle, title: tab.title, url: tab.url, owned: false }] });
    expect(mocks.scripting.executeScript).not.toHaveBeenCalled();
  });

  it("restores owned tabs without creating pages or changing the user's focus", async () => {
    mocks.storage.session.get.mockResolvedValue({ owners: { 10: { conversationId: "chat-a", created: false }, 11: { conversationId: "chat-b", created: false } }, active: { "chat-a": 10 } });
    const browser = controller(); await browser.restore();
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [{ id: handle }], activeId: handle, browser: "firefox" });
    expect(mocks.tabs.create).not.toHaveBeenCalled(); expect(mocks.tabs.update).not.toHaveBeenCalled();
    expect(mocks.storage.session.set).toHaveBeenCalledWith({ owners: { 10: { conversationId: "chat-a", created: false } }, active: { "chat-a": 10 } });
  });

  it("prevents another conversation commandeering an owned tab", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    await expect(browser.execute(request("attach", { id: handle }, "chat-b"))).rejects.toMatchObject({ code: "browser_tab_busy" });
    await expect(browser.execute(request("navigate", { id: handle, url: "https://example.net" }, "chat-b"))).rejects.toMatchObject({ code: "browser_tab_not_owned" });
    expect(mocks.tabs.update).not.toHaveBeenCalled();
  });

  it("prunes ownership and diagnostics without closing a user's tab", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    await browser.execute(request("prune", { retained: [] }, ""));
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [] });
    expect(mocks.tabs.remove).not.toHaveBeenCalled();
    expect(mocks.scripting.executeScript).toHaveBeenLastCalledWith(expect.objectContaining({ args: ["stop"], world: "MAIN" }));
  });

  it("fences a tab creation cancelled by disconnect without adopting or replaying it", async () => {
    let resolve!: (opened: typeof tab) => void;
    mocks.tabs.create.mockImplementation(() => new Promise(done => { resolve = done; }));
    const browser = controller(); await browser.restore();
    const opening = browser.execute(request("open", { url: "https://example.net" }));
    await vi.waitFor(() => expect(resolve).toBeDefined());
    const releasing = browser.releaseAll(); resolve({ ...tab, id: 20 });
    await expect(opening).rejects.toMatchObject({ code: "browser_outcome_unknown" }); await releasing;
    await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [] });
    expect(mocks.tabs.create).toHaveBeenCalledTimes(1); expect(mocks.tabs.remove).not.toHaveBeenCalled();
  });

  it("screenshots the requested tab without activating it or another window", async () => {
    mocks.tabs.captureTab.mockResolvedValue("data:image/png;base64,aGVsbG8=");
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    await expect(browser.execute(request("screenshot", { id: handle }))).resolves.toEqual({ data: "aGVsbG8=", url: tab.url });
    expect(mocks.tabs.captureTab).toHaveBeenCalledWith(10, { format: "png" }); expect(mocks.tabs.update).not.toHaveBeenCalled(); expect(mocks.windows.update).not.toHaveBeenCalled();
  });

  it("returns an explicit recoverable limitation for CDP and refuses internal pages", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    await expect(browser.execute(request("devtools", { id: handle, method: "DOM.getDocument" }))).rejects.toMatchObject({ code: "browser_unsupported_method", details: { cdp: false, browser: "firefox" } });
    await expect(browser.execute(request("open", { url: "about:config" }))).rejects.toMatchObject({ code: "browser_invalid_url" });
    await expect(browser.execute(request("navigate", { id: handle, url: "https://addons.mozilla.org/" }))).rejects.toMatchObject({ code: "browser_invalid_url" });
    expect(mocks.tabs.create).not.toHaveBeenCalled();
  });

  it("explains missing screenshot permission without capturing a different tab", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    const capture = mocks.tabs.captureTab; Reflect.deleteProperty(mocks.tabs, "captureTab");
    try {
      await expect(browser.execute(request("screenshot", { id: handle }))).rejects.toMatchObject({ code: "browser_screenshot_unavailable", message: expect.stringContaining("todos os sites") });
      expect(mocks.tabs.update).not.toHaveBeenCalled();
    } finally { mocks.tabs.captureTab = capture; }
  });

  it("surfaces page CSP refusal without repeating an evaluation", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    mocks.scripting.executeScript.mockRejectedValue(new Error("CSP unsafe-eval denied"));
    await expect(browser.execute(request("evaluate", { id: handle, expression: "window.title" }))).rejects.toMatchObject({ code: "browser_evaluation_failed" });
    expect(mocks.scripting.executeScript).toHaveBeenCalledTimes(2);
  });

  it("uses isolated world semantic snapshots and scopes element handles to their frame", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    mocks.scripting.executeScript.mockResolvedValue([{ frameId: 0, result: { elements: [{ id: "snapshot-a-1", role: "button", name: "Save" }] } }]);
    await expect(browser.execute(request("snapshot", { id: handle }))).resolves.toMatchObject({ frameId: "0", elements: [{ id: "snapshot-a-1" }], frames: [{ id: "0", available: true }] });
    expect(mocks.scripting.executeScript).toHaveBeenLastCalledWith(expect.objectContaining({ world: "ISOLATED", target: { tabId: 10, frameIds: [0] } }));
    await expect(browser.execute(request("click", { id: handle, element: "snapshot-a-1", frameId: "2" }))).rejects.toMatchObject({ code: "browser_invalid_request" });
    browser.updated(10, { status: "loading" });
    await expect(browser.execute(request("click", { id: handle, element: "snapshot-a-1" }))).rejects.toMatchObject({ code: "browser_stale_element" });
  });

  it("dispatches once and returns unknown outcome if the tab navigates during input", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    mocks.scripting.executeScript.mockResolvedValueOnce([{ frameId: 0, result: { ready: true, token: "prepared-a", tag: "button" } }]).mockRejectedValueOnce(new Error("frame detached"));
    await expect(browser.execute(request("click", { id: handle, locator: { role: "button", name: "Save" } }))).rejects.toMatchObject({ code: "browser_outcome_unknown", details: { dispatched: true } });
    expect(mocks.scripting.executeScript).toHaveBeenCalledTimes(3);
  });

  it("does not keep a tab queue blocked when a page evaluation never resolves", async () => {
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    vi.useFakeTimers();
    try {
      mocks.scripting.executeScript.mockImplementationOnce(() => new Promise(() => {}));
      const evaluating = expect(browser.execute(request("evaluate", { id: handle, expression: "new Promise(() => {})" }))).rejects.toMatchObject({ code: "browser_outcome_unknown" });
      await vi.advanceTimersByTimeAsync(20000); await evaluating;
      await expect(browser.execute(request("list"))).resolves.toMatchObject({ tabs: [{ id: handle }] });
    } finally { vi.useRealTimers(); }
  });
});

describe("Firefox diagnostic buffers", () => {
  it("records only owned tabs and forwards complete response bytes unchanged", async () => {
    const filter: ReturnType<typeof mocks.webRequest.filterResponseData> = stream();
    mocks.webRequest.filterResponseData.mockReturnValue(filter);
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    const before = mocks.webRequest.onBeforeRequest.addListener.mock.calls[0][0];
    before({ tabId: 11, requestId: "personal", method: "GET", url: "https://personal.example" });
    expect(mocks.webRequest.filterResponseData).not.toHaveBeenCalled();
    before({ tabId: 10, requestId: "request-a", method: "GET", url: "https://example.org/api", type: "xmlhttprequest" });
    mocks.webRequest.onHeadersReceived.addListener.mock.calls[0][0]({ tabId: 10, requestId: "request-a", method: "GET", url: "https://example.org/api", statusCode: 200, responseHeaders: [{ name: "Content-Type", value: "application/json" }] });
    await expect(browser.execute(request("response_body", { id: handle, requestId: "request-a" }))).rejects.toMatchObject({ code: "browser_response_not_ready" });
    const bytes = new TextEncoder().encode('{"answer":42}').buffer;
    filter.ondata?.({ data: bytes }); filter.onstop?.();
    expect(filter.write).toHaveBeenCalledWith(bytes); expect(filter.close).toHaveBeenCalledOnce();
    await expect(browser.execute(request("response_body", { id: handle, requestId: "request-a" }))).resolves.toEqual({ body: '{"answer":42}', base64Encoded: false, truncated: false, totalBytes: 13 });
    await expect(browser.execute(request("network", { id: handle }))).resolves.toMatchObject({ total: 1, requests: [{ id: "request-a", status: 200, type: "xmlhttprequest" }] });
  });

  it("bounds response storage while preserving a large response and disconnects on release", async () => {
    const filter: ReturnType<typeof mocks.webRequest.filterResponseData> = stream(); mocks.webRequest.filterResponseData.mockReturnValue(filter);
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    mocks.webRequest.onBeforeRequest.addListener.mock.calls[0][0]({ tabId: 10, requestId: "large", method: "GET", url: "https://example.org/api" });
    const first = new TextEncoder().encode("x".repeat(40000)).buffer, second = new TextEncoder().encode("y".repeat(40000)).buffer;
    filter.ondata?.({ data: first }); filter.ondata?.({ data: second }); filter.onstop?.();
    expect(filter.write).toHaveBeenNthCalledWith(1, first); expect(filter.write).toHaveBeenNthCalledWith(2, second);
    const result = await browser.execute(request("response_body", { id: handle, requestId: "large" }));
    expect(result).toMatchObject({ body: "x".repeat(40000) + "y".repeat(24000), truncated: true, totalBytes: 80000 });
    mocks.webRequest.onBeforeRequest.addListener.mock.calls[0][0]({ tabId: 10, requestId: "pending", method: "GET", url: "https://example.org/stream" });
    await browser.releaseAll(); expect(filter.disconnect).toHaveBeenCalledOnce();
    await expect(browser.execute(request("response_body", { id: handle, requestId: "large" }))).rejects.toMatchObject({ code: "browser_tab_not_owned" });
  });

  it("reports unsupported response capture without refetching a request", async () => {
    mocks.webRequest.filterResponseData.mockImplementation(() => { throw new Error("not allowed"); });
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    mocks.webRequest.onBeforeRequest.addListener.mock.calls[0][0]({ tabId: 10, requestId: "unavailable", method: "POST", url: "https://example.org/pay" });
    await expect(browser.execute(request("response_body", { id: handle, requestId: "unavailable" }))).rejects.toMatchObject({ code: "browser_response_unavailable" });
    expect(mocks.tabs.reload).not.toHaveBeenCalled(); expect(mocks.tabs.update).not.toHaveBeenCalled();
  });

  it("encodes binary bodies and expires only the oldest entries in its finite buffer", async () => {
    const filter: ReturnType<typeof mocks.webRequest.filterResponseData> = stream(); mocks.webRequest.filterResponseData.mockReturnValue(filter);
    const browser = controller(); await browser.restore(); await browser.execute(request("attach", { id: handle }));
    const before = mocks.webRequest.onBeforeRequest.addListener.mock.calls[0][0];
    before({ tabId: 10, requestId: "binary", method: "GET", url: "https://example.org/image.png" });
    mocks.webRequest.onHeadersReceived.addListener.mock.calls[0][0]({ tabId: 10, requestId: "binary", method: "GET", url: "https://example.org/image.png", responseHeaders: [{ name: "Content-Type", value: "image/png" }] });
    filter.ondata?.({ data: new Uint8Array([0, 255, 128]).buffer }); filter.onstop?.();
    await expect(browser.execute(request("response_body", { id: handle, requestId: "binary" }))).resolves.toMatchObject({ body: "AP+A", base64Encoded: true, totalBytes: 3 });
    for (let index = 0; index < 200; index++) before({ tabId: 10, requestId: `r-${index}`, method: "GET", url: `https://example.org/${index}` });
    await expect(browser.execute(request("response_body", { id: handle, requestId: "binary" }))).rejects.toMatchObject({ code: "browser_request_expired" });
    await expect(browser.execute(request("network", { id: handle }))).resolves.toMatchObject({ total: 200 });
    await browser.releaseAll();
  });
});

describe("Firefox page adapters", () => {
  it("records console events once, preserves original logging and removes hooks", () => {
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    firefoxConsole("start"); firefoxConsole("start"); console.log("hello", { answer: 42 });
    expect(firefoxConsole("read").logs).toEqual([{ level: "log", text: 'hello {"answer":42}', time: expect.any(Number) }]);
    expect(log).toHaveBeenCalledOnce(); firefoxConsole("stop"); console.log("after");
    expect(log).toHaveBeenCalledTimes(2); expect(firefoxConsole("read").logs).toEqual([]); log.mockRestore();
  });

  it("refuses unsupported synthetic keyboard default actions before dispatch", () => {
    const input = document.createElement("input"); document.body.append(input); input.focus();
    vi.spyOn(input, "getBoundingClientRect").mockReturnValue({ width: 100, height: 20, x: 0, y: 0, left: 0, right: 100, top: 0, bottom: 20, toJSON: () => ({}) });
    const windowState = window as unknown as { __jarvisPrepared?: { token: string; element: Element } };
    windowState.__jarvisPrepared = { token: "prepared-a", element: input };
    const keydown = vi.fn(); input.addEventListener("keydown", keydown);
    expect(firefoxInput({ action: "press", token: "prepared-a", key: "Tab" })).toMatchObject({ ok: false, dispatched: false, code: "browser_unsupported_input" });
    expect(keydown).not.toHaveBeenCalled();
  });

  it("updates controlled fields with input/change events and avoids detached targets", () => {
    const input = document.createElement("input"); document.body.append(input); input.focus();
    vi.spyOn(input, "getBoundingClientRect").mockReturnValue({ width: 100, height: 20, x: 0, y: 0, left: 0, right: 100, top: 0, bottom: 20, toJSON: () => ({}) });
    const windowState = window as unknown as { __jarvisPrepared?: { token: string; element: Element } };
    windowState.__jarvisPrepared = { token: "prepared-a", element: input };
    const update = vi.fn(); input.addEventListener("input", update); input.addEventListener("change", update);
    expect(firefoxInput({ action: "fill", token: "prepared-a", text: "hello" })).toMatchObject({ ok: true, trustedInput: false });
    expect(input.value).toBe("hello"); expect(update).toHaveBeenCalledTimes(2);
    windowState.__jarvisPrepared = { token: "prepared-b", element: input }; input.remove();
    expect(firefoxInput({ action: "fill", token: "prepared-b", text: "changed" })).toMatchObject({ ok: false, dispatched: false, code: "browser_stale_element" }); expect(input.value).toBe("hello");
  });

  it("supports synthetic arrow handlers in custom controls without claiming native default input", () => {
    const button = document.createElement("button"); document.body.append(button); button.focus();
    vi.spyOn(button, "getBoundingClientRect").mockReturnValue({ width: 100, height: 20, x: 0, y: 0, left: 0, right: 100, top: 0, bottom: 20, toJSON: () => ({}) });
    (window as unknown as { __jarvisPrepared?: { token: string; element: Element } }).__jarvisPrepared = { token: "prepared-arrow", element: button };
    button.addEventListener("keydown", event => { if (event.key === "ArrowDown") button.setAttribute("aria-expanded", "true"); });
    expect(firefoxInput({ action: "press", token: "prepared-arrow", key: "ArrowDown" })).toEqual({ ok: true, dispatched: true, trustedInput: false, nativeDefaultActions: false });
    expect(button.getAttribute("aria-expanded")).toBe("true");
  });

  it("activates pointer-driven controls once and avoids clicking a replacement target", () => {
    const button = document.createElement("button"); document.body.append(button);
    vi.spyOn(button, "getBoundingClientRect").mockReturnValue({ width: 100, height: 20, x: 0, y: 0, left: 0, right: 100, top: 0, bottom: 20, toJSON: () => ({}) });
    Object.defineProperty(document, "elementFromPoint", { configurable: true, value: () => button });
    const state = window as unknown as { __jarvisPrepared?: { token: string; element: Element } };
    state.__jarvisPrepared = { token: "pointer-a", element: button };
    const down = vi.fn(); const click = vi.fn(); button.addEventListener("pointerdown", down); button.addEventListener("click", click);
    expect(firefoxInput({ action: "click", token: "pointer-a" })).toMatchObject({ ok: true, dispatched: true, trustedInput: false });
    expect(down).toHaveBeenCalledOnce(); expect(click).toHaveBeenCalledOnce();
    state.__jarvisPrepared = { token: "pointer-b", element: button };
    button.addEventListener("pointerdown", () => button.remove(), { once: true });
    expect(firefoxInput({ action: "click", token: "pointer-b" })).toMatchObject({ ok: false, dispatched: true, code: "browser_outcome_unknown" });
    expect(click).toHaveBeenCalledOnce();
    Reflect.deleteProperty(document, "elementFromPoint");
  });
});
