import { readFileSync } from "node:fs";
import { createContext, runInContext } from "node:vm";
import ts from "typescript";
import { expect, it, vi } from "vitest";

interface Session { id: string; phase: string; pageUrl: string; sourceFile: string; expectedVariants: number; arrivedVariants: number }
const old: Session = { id: "794ffcde", phase: "generating", pageUrl: "/dashboard", sourceFile: "Dashboard.tsx", expectedVariants: 3, arrivedVariants: 3 };

function harness(pathname = "/login") {
  const source = readFileSync("src-tauri/resources/impeccable/.agents/skills/impeccable/scripts/live-browser.js", "utf8");
  const file = ts.createSourceFile("live.js", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const names = new Set(["jarvisOwnsSession", "syncJarvisLiveSession", "resumeJarvisLiveRoute", "findActiveSessionSummary", "findAdoptableServerSession", "serverSessionAsSavedShape", "restoreSessionWithoutWrapper", "restoreFromActiveSessions", "isTerminalSessionSummary", "normalizePagePath", "pageMatchesCurrent", "clampVariantIndex", "cleanupAcceptedSession", "togglePick", "toggleInsert"]);
  const declarations: string[] = [];
  function visit(node: ts.Node): void {
    if (ts.isFunctionDeclaration(node) && node.name && names.has(node.name.text)) declarations.push(node.getText(file));
    ts.forEachChild(node, visit);
  }
  visit(file);
  expect(declarations).toHaveLength(names.size);
  const location = { origin: "http://localhost:3001", pathname };
  const write = vi.fn();
  const clear = vi.fn();
  const removePreview = vi.fn();
  const context = createContext({ window: { __jarvisBrowser() {} }, document, location, URL, write, clear, removePreview });
  runInContext(`
    let jarvisLiveProjection = null, jarvisLiveSessions = null, jarvisLiveRoute = location.pathname, jarvisRestoreKey = null;
    let state = 'IDLE', currentSessionId = null, selectedElement = null, hoveredElement = null, variantObserver = null;
    let expectedVariants = 0, arrivedVariants = 0, visibleVariant = 0, currentPreviewFile = null, currentSourceFile = null, currentPreviewMode = null;
    let selectedCount = 3, selectedAction = 'impeccable', recoveryWaitingForAnchor = false, pickedAnchorSnapshot = null;
    let pagePickSkipClick = false, parameterGenerationState = 'idle', sessionOrigin = null, editBadgeSuppressed = false;
    let parameterReadyAnnouncedSession = null, pendingAcceptedSession = null, liveInteractionRevision = 0, svelteComponentSession = null;
    let pickActive = false, insertActive = false, pendingApplyInFlight = false, configureKind = 'replace', cached = null;
    const ADOPTABLE_SESSION_PHASES = new Set(['generate_requested', 'generating', 'cycling', 'variants_ready']);
    function setLiveState(next) { state = next; }
    function loadSession() { return cached; }
    function clearSession() { clear(); cached = null; }
    function saveSession() {} function isSessionHandled() { return false; }
    function findAnyVariantsWrapper() { return null; }
    function resumeSession() { throw new Error('No wrapper in this fixture'); }
    function recoverMissedGenerationCompletion() {} function isFrameworkComponentPreviewMode() { return false; }
    function fetchAgentPollingStatus() {}
    function applySavedSessionMeta(value) { currentSourceFile = value.sourceFile || currentSourceFile; }
    function findLiveElementFromAnchorSnapshot() { return null; }
    function showBar() {} function startScrollTracking() {} function startVariantObserver() { return null; }
    function injectVariantsFromSource() {} function queueCheckpoint(reason) { write(reason); }
    function hideBar() {} function hideHighlight() {} function stopScrollTracking() {} function stopScrollLock() {}
    function removeVariantStateStylesheet() { removePreview(); }
    function clearScrollY() {} function resetSessionFileMeta() { currentSourceFile = null; currentPreviewFile = null; }
    function renderEditBadge() {} function hideShaderOverlay() {}
    function showManualApplyBusyToast() {} function clearInsertPicking() {} function saveInteractionPrefs() {}
    function updateGlobalBarState() {} function teardownConfigureChrome() {} function hideActionPicker() {}
    function syncPageChatFocus() {} function cancelInsertConfigure() {}
    ${declarations.join("\n")}
    globalThis.live = {
      sync: ids => syncJarvisLiveSession({ sessionIds: ids }),
      sse: sessions => restoreFromActiveSessions(sessions, 'sse_connected'),
      connect: sessions => { jarvisLiveSessions = sessions; resumeJarvisLiveRoute(); },
      cache: value => { cached = value; },
      pick: togglePick, insert: toggleInsert,
      snapshot: () => ({ state, currentSessionId, pickActive, insertActive, revision: liveInteractionRevision, cached })
    };
  `, context);
  const live = context.live as {
    sync(ids: string[]): void; sse(sessions: Session[]): void; connect(sessions: Session[]): void; cache(value: object): void;
    pick(): void; insert(): void;
    snapshot(): { state: string; currentSessionId: string | null; pickActive: boolean; insertActive: boolean; revision: number; cached: object | null };
  };
  return { live, location, write, clear, removePreview };
}

it("leaves dashboard selection and insertion usable without reviving an old request", () => {
  const { live, location, write } = harness();
  live.sync([]);
  location.pathname = "/dashboard";
  live.cache({ id: old.id, state: "GENERATING", pageUrl: "/dashboard" });
  live.connect([old]); // Canonical history is not proof that Jarvis owns this request.
  live.sync([]);
  live.pick();
  expect(live.snapshot()).toMatchObject({ state: "PICKING", pickActive: true, currentSessionId: null });
  live.insert();
  expect(live.snapshot()).toMatchObject({ state: "PICKING", pickActive: false, insertActive: true });
  expect(write).not.toHaveBeenCalled();
});

it("waits for the native queue on a reload and restores only the owned comparison once", () => {
  const { live, write } = harness("/dashboard");
  const current = { ...old, id: "current" };
  live.sse([old, current]);
  expect(live.snapshot().currentSessionId).toBeNull();
  live.sync([current.id]);
  live.sync([current.id]);
  expect(live.snapshot()).toMatchObject({ state: "GENERATING", currentSessionId: current.id });
  expect(write).toHaveBeenCalledOnce();
});

it("frees another SPA route without discarding or exposing an unfinished preview, and restores it on return", () => {
  const { live, location, write, clear, removePreview } = harness();
  const current = { ...old, id: "current", pageUrl: "/login" };
  live.sync([current.id]);
  live.connect([current]);
  live.cache({ id: current.id, state: "CYCLING", pageUrl: "/login", visible: 2 });
  location.pathname = "/dashboard";
  live.sync([current.id]);
  expect(live.snapshot()).toMatchObject({ state: "PICKING", currentSessionId: null, cached: { visible: 2 } });
  expect(clear).not.toHaveBeenCalled();
  expect(removePreview).not.toHaveBeenCalled();
  expect(write).toHaveBeenCalledOnce();
  location.pathname = "/login";
  live.sync([current.id]);
  expect(live.snapshot().currentSessionId).toBeNull();
  live.connect([current]);
  expect(live.snapshot().currentSessionId).toBe(current.id);
  expect(write).toHaveBeenCalledTimes(2);
});

it("does not restore a comparison completed while the user was on another route", () => {
  const { live, location, write } = harness();
  const current = { ...old, id: "current", pageUrl: "/login" };
  live.sync([current.id]);
  live.connect([current]);
  location.pathname = "/dashboard";
  live.sync([current.id]);
  location.pathname = "/login";
  live.sync([current.id]);
  live.connect([]);
  live.pick();
  expect(live.snapshot()).toMatchObject({ state: "PICKING", currentSessionId: null });
  expect(write).toHaveBeenCalledOnce();
});
