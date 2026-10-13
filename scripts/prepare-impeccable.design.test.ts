import { readFile } from "node:fs/promises";
import path from "node:path";
import { createContext, runInContext } from "node:vm";
import ts from "typescript";
import { afterEach, expect, it, vi } from "vitest";
import { IMPECCABLE_RESOURCE_DIRECTORY, SKILL_DIRECTORY } from "./prepare-impeccable";

const event = "jarvis-live:design-changed";
const cleanups: (() => void)[] = [];
afterEach(() => { cleanups.splice(0).forEach(cleanup => cleanup()); document.body.innerHTML = ""; });

async function harness(open = true) {
  const source = await readFile(path.join(IMPECCABLE_RESOURCE_DIRECTORY, SKILL_DIRECTORY, "scripts/live-browser.js"), "utf8");
  const file = ts.createSourceFile("live.js", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const declarations: string[] = [];
  const listeners: string[] = [];
  function visit(node: ts.Node) {
    if (ts.isFunctionDeclaration(node) && node.name && ["fetchDesignSystem", "toggleDesignPanel", "refreshDesignSystem", "stopDesignRefresh"].includes(node.name.text)) declarations.push(node.getText(file));
    if (ts.isExpressionStatement(node) && ts.isCallExpression(node.expression) && node.expression.expression.getText(file) === "window.addEventListener" && node.expression.arguments[0] && ts.isStringLiteral(node.expression.arguments[0]) && node.expression.arguments[0].text === event) listeners.push(node.getText(file));
    ts.forEachChild(node, visit);
  }
  visit(file);
  document.body.innerHTML = '<main id="preview">Alteração em andamento</main><output id="design"></output>';
  let name: string | null = null;
  const fetch = vi.fn(async (url: string) => url.includes(".json")
    ? new Response(JSON.stringify({ present: name !== null, hasMd: name !== null, parsed: { frontmatter: { name } } }))
    : new Response(name === null ? "Not found" : `# ${name}`, { status: name === null ? 404 : 200 }));
  const bindings = new Map<string, EventListener>();
  const host = {
    addEventListener(name: string, callback: EventListener) { bindings.set(name, callback); window.addEventListener(name, callback); },
    removeEventListener(name: string, callback: EventListener) { bindings.delete(name); window.removeEventListener(name, callback); },
  };
  cleanups.push(() => { for (const [name, callback] of bindings) window.removeEventListener(name, callback); });
  const context = createContext({ window: host, fetch, document });
  runInContext(`
    const PORT = 8400, TOKEN = 'fixture';
    let designRequest = 0;
    let designState = { open: ${open}, present: false, loading: false, error: null, raw: null, parsed: null };
    let state = 'GENERATING', currentSessionId = 'active-variant', pendingApplyInFlight = false;
    function renderDesignBody() {}
    function renderDesignChrome() { document.getElementById('design').textContent = designState.raw || 'Documento ausente'; }
    function updateGlobalBarState() {}
    function showManualApplyBusyToast() { throw new Error('An active alteration was interrupted'); }
    ${declarations.join("\n")}
    ${listeners.join("\n")}
    globalThis.live = { snapshot: () => ({ state, currentSessionId, raw: designState.raw, present: designState.present }), toggle: () => toggleDesignPanel(), read: () => fetchDesignSystem(), dispose: () => typeof stopDesignRefresh === 'function' && stopDesignRefresh() };
  `, context);
  const live = runInContext("live", context) as { snapshot(): { state: string; currentSessionId: string; raw: string | null; present: boolean | null }; toggle(): void; read(): Promise<void>; dispose(): void };
  return { live, fetch, create(value: string) { name = value; window.dispatchEvent(new Event(event)); } };
}

it("recognizes a newly created design in the open panel while preserving the ongoing alteration", async () => {
  const { live, fetch, create } = await harness();
  const preview = document.getElementById("preview");
  create("Novo design");
  await vi.waitFor(() => expect(document.getElementById("design")).toHaveTextContent("Novo design"));
  expect(live.snapshot()).toMatchObject({ state: "GENERATING", currentSessionId: "active-variant", present: true });
  expect(document.getElementById("preview")).toBe(preview);
  expect(fetch.mock.calls.every(([url]) => url.includes("/design-system"))).toBe(true);
});

it("invalidates a closed empty panel and reads a newly linked design when it opens", async () => {
  const { live, fetch, create } = await harness(false);
  create("Documento vinculado");
  expect(fetch).not.toHaveBeenCalled();
  live.toggle();
  await vi.waitFor(() => expect(document.getElementById("design")).toHaveTextContent("Documento vinculado"));
});

it("replaces the visible document when another existing design is linked", async () => {
  const { create } = await harness();
  create("Design original");
  await vi.waitFor(() => expect(document.getElementById("design")).toHaveTextContent("Design original"));
  create("Design escolhido");
  await vi.waitFor(() => expect(document.getElementById("design")).toHaveTextContent("Design escolhido"));
});

it("keeps the latest linked design when an older read finishes later", async () => {
  const { live, fetch, create } = await harness();
  let finish: ((response: Response) => void) | undefined;
  fetch.mockImplementationOnce(() => new Promise<Response>(resolve => { finish = resolve; }));
  const original = live.read();
  create("Design mais recente");
  await vi.waitFor(() => expect(document.getElementById("design")).toHaveTextContent("Design mais recente"));
  finish?.(new Response(JSON.stringify({ present: false })));
  await original;
  expect(document.getElementById("design")).toHaveTextContent("Design mais recente");
});

it("stops listening and invalidates pending reads when Live is disposed", async () => {
  const { live, fetch, create } = await harness();
  create("Design inicial");
  await vi.waitFor(() => expect(document.getElementById("design")).toHaveTextContent("Design inicial"));
  live.dispose();
  fetch.mockClear();
  create("Outra sessão");
  expect(fetch).not.toHaveBeenCalled();
  expect(document.getElementById("design")).toHaveTextContent("Design inicial");
});
