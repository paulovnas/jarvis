import { readFile } from "node:fs/promises";
import path from "node:path";
import { createContext, runInContext } from "node:vm";
import ts from "typescript";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { bindNativeAcceptLifecycle, IMPECCABLE_RESOURCE_DIRECTORY, SKILL_DIRECTORY } from "./prepare-impeccable";

const browserPath = path.join(IMPECCABLE_RESOURCE_DIRECTORY, SKILL_DIRECTORY, "scripts");
const sessionId = "b0338dda";
interface LiveMessage { type: string; id: string; data?: { carbonize?: boolean }; message?: string; activeSessions?: { id: string; phase: string }[] }
interface AcceptHarness {
  accept(): Promise<void>;
  deliver(message: LiveMessage): void;
  restore(activeSessions?: { id: string; phase: string }[]): boolean;
  snapshot(): { state: string; id: string | null; pending: { id: string; variant: string } | null; handled: boolean };
}

/** Execute the shipped algorithms, stubbing only transport and unrelated chrome. */
async function harness(framework = true, deferredPost = false): Promise<{
  live: AcceptHarness;
  posted: { type: string; id: string; variantId: string }[];
  admit(index?: number): void;
  rejectPost(index?: number): void;
  reloadCount(): number;
}> {
  const shipped = await readFile(path.join(browserPath, "live-browser.js"), "utf8");
  const source = shipped.includes("HTTP success only confirms durable queue admission.") ? shipped : bindNativeAcceptLifecycle(shipped);
  const modules = [source, await readFile(path.join(browserPath, "live-browser-session.js"), "utf8"), await readFile(path.join(browserPath, "live-browser-dom.js"), "utf8")];
  const declarations = new Map<string, string>();
  for (const content of modules) {
    const file = ts.createSourceFile("live.js", content, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
    function visit(node: ts.Node): void {
      if (ts.isFunctionDeclaration(node) && node.name && !declarations.has(node.name.text)) declarations.set(node.name.text, node.getText(file));
      ts.forEachChild(node, visit);
    }
    visit(file);
  }
  const names = [
    "handleAccept", "maybeCompleteAcceptedSession", "scheduleAcceptCleanup", "cleanupAcceptedSession",
    "snapshotAcceptedVariantDom", "selectorForAcceptedRoot", "acceptedDomAlreadyClean", "ensureAcceptedDomClean",
    "findAcceptedRuntimeWrappers", "restoreAcceptedDomFromSnapshot", "reloadAfterMissingAcceptedDom",
    "findVariantsWrapper", "pickPopulatedVariantsWrapper", "readVisibleVariantFromDOM", "isVariantShown",
    "variantStateSelector", "variantParamDecls", "updateVariantStateStylesheet", "removeVariantStateStylesheet",
    "deferredRecoverySuperseded", "cssIdent", "hasFrameworkHmrOwnership", "connectSSE", "sendEvent", "maybeCompleteSteer",
    "createLiveBrowserSessionState", "saveSession", "clearSession", "markSessionHandled", "isSessionHandled", "clearScrollY",
    "watchForHandledRuntimeWrapper", "scheduleHandledRuntimeWrapperReload", "handledWrapperReloadKey", "clearHandledWrapperReloadStamp",
    "restoreSessionWithoutWrapper", "findActiveSessionSummary", "isTerminalSessionSummary", "jarvisOwnsSession",
  ];
  document.body.innerHTML = `<main><div data-impeccable-variants="${sessionId}"><div data-impeccable-variant="1"><section class="chosen">One</section></div><div data-impeccable-variant="2"><section class="chosen">Two</section></div><div data-impeccable-variant="3"><section class="chosen">Three</section></div></div></main>`;
  if (framework) Object.defineProperty(document.querySelector("main"), "__reactFiber$fixture", { value: {} });
  const posted: { type: string; id: string; variantId: string }[] = [];
  const admissions: { admit(): void; reject(): void }[] = [];
  const fetch = vi.fn((_url: string, options: RequestInit) => {
    posted.push(JSON.parse(String(options.body)) as { type: string; id: string; variantId: string });
    if (!deferredPost) return Promise.resolve(new Response("{}"));
    return new Promise<Response>((resolve, reject) => {
      admissions.push({ admit: () => resolve(new Response("{}")), reject: () => reject(new Error("HTTP connection interrupted")) });
    });
  });
  const reload = vi.fn();
  const context = createContext({
    document, window: { CSS: window.CSS }, localStorage, sessionStorage, getComputedStyle, MutationObserver,
    location: { pathname: "/dashboard", hash: "", search: "", reload },
    setTimeout, clearTimeout, queueMicrotask: (callback: () => void) => Promise.resolve().then(callback), fetch, console: { ...console, error: vi.fn() },
    EventSource: class { onmessage?: (event: { data: string }) => void; onopen?: () => void; onerror?: () => void },
  });
  runInContext(`
    const TOKEN = 'fixture', PORT = 8400, AGENT_TARGET_CLIENT_ID = 'fixture', APP_ROOT = '/fixture';
    const VARIANT_STATE_STYLE_ID = 'impeccable-variant-state', HANDLED_WRAPPER_RELOAD_KEY = 'fixture-reload';
    const VARIANT_HIDE_DECL = 'display: none !important;', VARIANT_SHOW_DECL = 'display: block !important;';
    const sessionState = createLiveBrowserSessionState({ prefix: 'fixture', storage: localStorage });
    const handledRuntimeWrapperWatchers = new Map(), handledRuntimeWrapperReloadSessions = new Set();
    let state = 'CYCLING', currentSessionId = '${sessionId}', visibleVariant = 2, arrivedVariants = 3, expectedVariants = 3;
    let pendingAcceptedSession = null, awaitingAcceptResult = null, pendingApplyInFlight = false, variantSelectionPromise = null;
    let svelteComponentSession = null, paramsCurrentValues = {}, liveInteractionRevision = 0;
    let currentSourceFile = 'App.tsx', currentPreviewFile = null, currentPreviewMode = null, selectedAction = 'impeccable', selectedCount = 3;
    let parameterGenerationState = 'idle', sessionOrigin = null, insertPlaceholderSnapshot = null, pickedAnchorSnapshot = null, pickedAnchorViewportTop = null;
    let selectedElement = null, hoveredElement = null, pagePickSkipClick = false, editBadgeSuppressed = false, parameterReadyAnnouncedSession = null;
    let variantObserver = null, evtSource = null, sseRetries = 0, sessionCreationGate = Promise.resolve(), steerRequestId = null;
    let hasProjectContext = true, liveBarHiddenByHelper = false, pickActive = false, insertActive = false;
    const IMPECCABLE_COMMAND = 'impeccable';
    const SSE_MAX_RETRIES = 20;
    function setLiveState(next) { state = next; }
    function updateBarContent() {} function showOrUpdateCyclingBar() {} function showToast() {} function hideShaderOverlay() {}
    function hideBar() {} function hideHighlight() {} function stopScrollTracking() {} function stopScrollLock() {} function renderEditBadge() {}
    function resetSessionFileMeta() { currentSourceFile = null; currentPreviewFile = null; currentPreviewMode = null; }
    function isFrameworkComponentPreviewMode() { return false; }
    function rememberSessionFileMeta() {} function completeParameterGenerationIfReady() {}
    function applyLiveBarPreference() {} function syncAgentPollingUi() {} function startAgentStatusPoll() {}
    function restoreFromActiveSessions() {} function recoverMissedGenerationCompletion() {}
    function syncPageInteractionCursor() {} function syncPageChatFocus() {}
    function loadSession() { return { id: '${sessionId}', state: 'CYCLING', pageUrl: '/dashboard' }; }
    function pageMatchesCurrent(url) { return url === '/dashboard'; }
    function findAdoptableServerSession() { return null; }
    ${names.map((name) => {
      const declaration = declarations.get(name);
      if (!declaration) throw new Error(`Missing shipped Live algorithm ${name}`);
      return declaration;
    }).join("\n")}
    updateVariantStateStylesheet(currentSessionId, visibleVariant);
    connectSSE();
    globalThis.live = {
      accept: handleAccept,
      restore: activeSessions => restoreSessionWithoutWrapper('test-reconnect', activeSessions),
      deliver: message => evtSource.onmessage({ data: JSON.stringify(message) }),
      snapshot: () => ({ state, id: currentSessionId, pending: pendingAcceptedSession, handled: sessionState.isHandled('${sessionId}') })
    };
  `, context);
  return {
    live: context.live as AcceptHarness, posted, reloadCount: () => reload.mock.calls.length,
    admit: (index = 0) => admissions[index].admit(), rejectPost: (index = 0) => admissions[index].reject(),
  };
}

async function settle(): Promise<void> { for (let i = 0; i < 12; i++) await Promise.resolve(); }
function expectOnlySecondVisible(): void {
  expect([...document.querySelectorAll("[data-impeccable-variant]")].filter((element) => getComputedStyle(element).display !== "none").map((element) => element.textContent)).toEqual(["Two"]);
}
function reconcileHmr(): void {
  const wrapper = document.querySelector(`[data-impeccable-variants="${sessionId}"]`);
  if (!wrapper) throw new Error("Expected a mounted comparison");
  const accepted = document.createElement("section");
  accepted.className = "chosen";
  accepted.textContent = "Two";
  wrapper.replaceWith(accepted);
}

beforeEach(() => { vi.useFakeTimers(); localStorage.clear(); sessionStorage.clear(); });
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); document.body.replaceChildren(); document.querySelector("#impeccable-variant-state")?.remove(); });

describe("native Live accept acknowledgement", () => {
  it("does not resurrect a cached comparison before connection or after the canonical list is empty", async () => {
    const { live, posted } = await harness();
    expect(live.restore()).toBe(false);
    expect(live.restore([])).toBe(false);
    expect(posted).toHaveLength(0);
    expect(live.snapshot().state).toBe("CYCLING");
  });

  it("settles a restored comparison from canonical completion without another accept request", async () => {
    const { live, posted } = await harness();
    live.deliver({ type: "complete", id: "another-comparison" });
    expect(live.snapshot().state).toBe("CYCLING");
    reconcileHmr();
    live.deliver({ type: "complete", id: sessionId });
    await settle();
    expect(live.snapshot()).toMatchObject({ state: "PICKING", id: null, handled: true });
    expect(posted).toHaveLength(0);
    expect(document.querySelector("main")?.textContent).toBe("Two");
  });

  it("reloads a restored comparison after completion without exposing its other alternatives", async () => {
    const { live, reloadCount } = await harness();
    live.deliver({ type: "complete", id: sessionId });
    await settle();
    expect(reloadCount()).toBe(1);
    expect(live.snapshot().handled).toBe(true);
    expectOnlySecondVisible();
  });

  it("reconciles completion missed while disconnected, preserving an accept still pending on the server", async () => {
    const { live } = await harness();
    live.deliver({ type: "connected", id: "", activeSessions: [{ id: sessionId, phase: "accept_requested" }] });
    expect(live.snapshot()).toMatchObject({ state: "CYCLING", handled: false });
    reconcileHmr();
    live.deliver({ type: "connected", id: "", activeSessions: [] });
    await settle();
    expect(live.snapshot()).toMatchObject({ state: "PICKING", id: null, handled: true });
    expect(document.querySelector("main")?.textContent).toBe("Two");
  });

  it("keeps variant two until real completion and reconciled HMR, ignoring admission and partial acknowledgements", async () => {
    const { live, posted } = await harness();
    await live.accept();
    await settle();
    expect(posted).toMatchObject([{ type: "accept", id: sessionId, variantId: "2" }]);
    expect(live.snapshot()).toMatchObject({ state: "SAVING", id: sessionId, handled: false, pending: { variant: "2" } });
    expectOnlySecondVisible();
    live.deliver({ type: "accept", id: sessionId });
    live.deliver({ type: "done", id: sessionId });
    live.deliver({ type: "agent_done", id: sessionId, data: { carbonize: true } });
    live.deliver({ type: "complete", id: "another-comparison" });
    await settle();
    expect(live.snapshot().state).toBe("SAVING");
    expectOnlySecondVisible();
    live.deliver({ type: "complete", id: sessionId });
    await settle();
    expect(live.snapshot()).toMatchObject({ state: "CONFIRMED", id: sessionId, handled: true });
    expectOnlySecondVisible();
    reconcileHmr();
    await settle();
    expect(live.snapshot()).toMatchObject({ state: "PICKING", id: null, pending: null });
    expect(document.querySelector("#impeccable-variant-state")).toBeNull();
    expect(document.querySelector("main")?.textContent).toBe("Two");
  });

  it("preserves the chosen comparison on accept error and retries without exposing other variants", async () => {
    const { live, posted } = await harness();
    await live.accept(); await settle();
    live.deliver({ type: "error", id: sessionId, message: "Provider interrupted" });
    expect(live.snapshot()).toMatchObject({ state: "CYCLING", pending: null, handled: false });
    expectOnlySecondVisible();
    await live.accept(); await settle();
    expect(posted).toHaveLength(2);
    expect(posted[1].variantId).toBe("2");
    live.deliver({ type: "complete", id: sessionId });
    await settle(); expectOnlySecondVisible();
    reconcileHmr(); await settle();
    expect(live.snapshot().state).toBe("PICKING");
  });

  it("does not release a static comparison before completion and promotes only the chosen fallback afterwards", async () => {
    const { live } = await harness(false);
    await live.accept(); await settle();
    await vi.advanceTimersByTimeAsync(2000);
    expectOnlySecondVisible();
    expect(document.querySelectorAll("[data-impeccable-variant]")).toHaveLength(3);
    live.deliver({ type: "complete", id: sessionId });
    await settle(); expectOnlySecondVisible();
    await vi.advanceTimersByTimeAsync(1200);
    await settle();
    expect(document.querySelector("main")?.textContent).toBe("Two");
    expect(document.querySelectorAll("[data-impeccable-variant]")).toHaveLength(0);
    expect(live.snapshot().state).toBe("PICKING");
  });

  it("does not resurrect a completed comparison when its HTTP acknowledgement arrives later", async () => {
    const { live, admit, posted } = await harness(true, true);
    await live.accept(); await settle();
    expect(posted).toHaveLength(1);
    live.deliver({ type: "complete", id: sessionId });
    reconcileHmr(); await settle();
    expect(live.snapshot().state).toBe("PICKING");
    admit(); await settle();
    expect(live.snapshot()).toMatchObject({ state: "PICKING", id: null, pending: null });
  });

  it("ignores a late HTTP rejection after canonical completion and HMR cleanup", async () => {
    const { live, rejectPost } = await harness(true, true);
    await live.accept(); await settle();
    live.deliver({ type: "complete", id: sessionId });
    reconcileHmr(); await settle();
    rejectPost(); await settle();
    expect(live.snapshot()).toMatchObject({ state: "PICKING", id: null, pending: null, handled: true });
    expect(document.querySelector("main")?.textContent).toBe("Two");
  });

  it("does not let a rejected old HTTP attempt erase a newer retry with the same session ID", async () => {
    const { live, posted, admit, rejectPost } = await harness(true, true);
    await live.accept(); await settle();
    live.deliver({ type: "error", id: sessionId, message: "Provider interrupted" });
    await live.accept(); await settle();
    expect(posted).toHaveLength(2);
    rejectPost(0); await settle();
    expect(live.snapshot()).toMatchObject({ state: "SAVING", pending: { id: sessionId, variant: "2" }, handled: false });
    expectOnlySecondVisible();
    admit(1); await settle();
    live.deliver({ type: "complete", id: sessionId });
    reconcileHmr(); await settle();
    expect(live.snapshot()).toMatchObject({ state: "PICKING", id: null, pending: null });
  });

  it("retains visibility throughout bounded framework recovery instead of mutating React wrappers", async () => {
    const { live, reloadCount } = await harness();
    await live.accept(); await settle();
    live.deliver({ type: "complete", id: sessionId });
    await vi.advanceTimersByTimeAsync(1200);
    expectOnlySecondVisible();
    expect(document.querySelectorAll("[data-impeccable-variant]")).toHaveLength(3);
    await vi.advanceTimersByTimeAsync(10800);
    expect(reloadCount()).toBeGreaterThan(0);
    expectOnlySecondVisible();
    expect(live.snapshot()).toMatchObject({ state: "CONFIRMED", id: sessionId });
    expect(document.querySelectorAll("[data-impeccable-variant]")).toHaveLength(3);
  });

  it("fails closed when the pinned upstream accept contract changes", () => {
    expect(() => bindNativeAcceptLifecycle("if (typeof window === 'undefined') return; function handleAccept() { return true; }")).toThrow("contrato de aceitação Live mudou");
  });
});
