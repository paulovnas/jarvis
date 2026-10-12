import { readFileSync } from "node:fs";
import ts from "typescript";
import { afterEach, expect, it, vi } from "vitest";
import { pageOperation } from "./page";
import { frameOwnerOperation } from "./frames";

// The native CDP backend uses the extension's existing isolated-world guards.
const sourceFiles = ["browser-extension/page.ts", "browser-extension/frames.ts"];
const bundled = readFileSync("src-tauri/src/agent/browser/page-cdp.js", "utf8");

afterEach(() => {
  const guard = Reflect.get(window, "__jarvisGuard") as { cleanup: () => void } | undefined;
  guard?.cleanup();
  vi.restoreAllMocks();
  document.body.innerHTML = "";
  for (const key of ["__jarvisElements", "__jarvisPrepared", "__jarvisGuard", "__jarvisSequence", "__jarvisFrameGuards"]) Reflect.deleteProperty(window, key);
});

it("paginates the shipped Chromium snapshot once beyond the former native element limit", () => {
  const shared = new Function(`return (${bundled});`)() as { page: typeof pageOperation };
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 0, y: 0, width: 100, height: 24 }));
  document.body.innerHTML = Array.from({ length: 350 }, (_, index) => `<button>Item ${index}</button>`).join("");
  const result = shared.page({ action: "snapshot", generation: "native-document", offset: 270, limit: 6 });
  expect(result).toMatchObject({ total: 350, offset: 270, limit: 6 });
  if (!("elements" in result) || !result.elements) throw new Error("Native snapshot omitted elements");
  expect(result.elements).toHaveLength(6);
  expect(result.elements?.[0]).toMatchObject({ id: "native-document-271", name: "Item 270" });
  shared.page({ action: "snapshot", generation: "next-document", limit: 6 });
  expect(shared.page({ action: "prepare", element: "native-document-271", mode: "click" })).toMatchObject({ ready: false, code: "browser_stale_element" });
});

it("keeps the native page payload identical to the shared locator, actionability and frame guards", () => {
  const functions = sourceFiles.map(path => ts.transpileModule(readFileSync(path, "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, removeComments: true },
  }).outputText.replace(/^export /gm, "")).join("\n");
  expect(bundled).toBe(`// Generated from browser-extension/page.ts and frames.ts; checked by native-page.test.ts.\n(() => {\n${functions}\nreturn { page: pageOperation, frameOwner: frameOwnerOperation };\n})()\n`);
});

it("runs the shipped native payload and preserves ambiguous-target and expired-ID errors", () => {
  const shared = new Function(`return (${bundled});`)() as { page: typeof pageOperation; frameOwner: typeof frameOwnerOperation };
  document.body.innerHTML = '<button>Salvar</button><button>Salvar</button>';
  expect(shared.page({ action: "wait", locator: { role: "button", name: "Salvar" }, state: "visible" })).toMatchObject({ ready: false, code: "browser_ambiguous_element" });
  expect(shared.page({ action: "prepare", element: "expired-id", mode: "click" })).toMatchObject({ ready: false, code: "browser_stale_element" });
  expect(shared.page({ action: "inspect" })).toMatchObject({ title: document.title, url: location.href });
});

it("never turns a native readonly wait into a dispatched click or input", () => {
  const shared = new Function(`return (${bundled});`)() as { page: typeof pageOperation };
  document.body.innerHTML = '<button data-testid="save">Salvar</button>';
  let clicks = 0;
  document.querySelector("button")?.addEventListener("click", () => { clicks++; });
  expect(shared.page({ action: "wait", locator: { testId: "save" }, state: "attached" })).toMatchObject({ ready: true });
  expect(shared.page({ action: "wait", locator: { testId: "missing" }, state: "detached" })).toMatchObject({ ready: true });
  expect(clicks).toBe(0);
});

it("confirms Chromium carousel clicks that replace their target without accepting pre-click detachments", () => {
  const shared = new Function(`return (${bundled});`)() as { page: typeof pageOperation };
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 20, y: 20, width: 100, height: 24 }));
  document.body.innerHTML = '<button aria-label="Próxima variante">→</button>';
  const target = document.querySelector("button")!;
  const prepare = { action: "prepare" as const, locator: { role: "button", name: "Próxima variante" }, mode: "click" as const };
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: () => document.querySelector("button") });
  shared.page(prepare);
  const prepared = shared.page(prepare);
  if (!("token" in prepared) || !prepared.token) throw new Error("Native click was not prepared");
  const listeners = vi.spyOn(document, "addEventListener");
  expect(shared.page({ action: "guard", element: prepared.token })).toEqual({ ready: true });
  const capture = listeners.mock.calls.find(([type]) => type === "click")?.[1];
  if (typeof capture !== "function") throw new Error("Native click guard was not installed");
  const event = { type: "click", isTrusted: true, composedPath: () => [target, document], preventDefault: vi.fn(), stopImmediatePropagation: vi.fn() } as unknown as MouseEvent;
  capture(event);
  target.replaceWith(target.cloneNode(true));
  expect(event.preventDefault).not.toHaveBeenCalled();
  expect(shared.page({ action: "finish", element: prepared.token })).toEqual({ blocked: false });
  shared.page(prepare);
  const next = shared.page(prepare);
  if (!("token" in next) || !next.token) throw new Error("Replacement click was not prepared");
  shared.page({ action: "guard", element: next.token });
  document.querySelector("button")?.remove();
  expect(shared.page({ action: "finish", element: next.token })).toEqual({ blocked: true });
});
