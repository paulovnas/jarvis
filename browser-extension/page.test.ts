import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { pageOperation, type PageOperation } from "./page";

type PageState = { __jarvisElements?: unknown; __jarvisPrepared?: unknown; __jarvisGuard?: { cleanup: () => void } };
const page = window as unknown as PageState;
let hit: Element | null;
let boxes: WeakMap<Element, DOMRect>;
const box = (x = 20, y = 20, width = 120, height = 32) => new DOMRect(x, y, width, height);
const snapshot = (values: Partial<PageOperation> = {}) => {
  const result = pageOperation({ action: "snapshot", generation: "first", ...values });
  if (!("elements" in result) || !result.elements) throw new Error("Expected snapshot");
  return { ...result, elements: result.elements };
};
const prepared = (element: string, mode: PageOperation["mode"] = "click") => {
  const result = pageOperation({ action: "prepare", element, mode });
  return { ...result, token: "token" in result ? result.token : undefined };
};
const target = (html: string) => {
  document.body.innerHTML = html;
  const element = document.body.firstElementChild;
  if (!element) throw new Error("Expected target element");
  hit = element;
  return { element, id: snapshot().elements![0].id };
};
const capturedClick = () => {
  const events = vi.spyOn(document, "addEventListener");
  return () => {
    const listener = events.mock.calls.find(([type]) => type === "click")?.[1];
    if (typeof listener !== "function") throw new Error("Expected click guard");
    return listener;
  };
};
const trustedClick = (target: Element, path: EventTarget[] = [target, document]) => ({
  isTrusted: true, composedPath: () => path,
  preventDefault: vi.fn(), stopImmediatePropagation: vi.fn(),
} as unknown as MouseEvent);

beforeEach(() => {
  document.body.innerHTML = "";
  document.title = "Teste";
  delete page.__jarvisElements;
  delete page.__jarvisPrepared;
  page.__jarvisGuard?.cleanup();
  delete page.__jarvisGuard;
  boxes = new WeakMap();
  hit = null;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) { return boxes.get(this) ?? box(); });
  vi.stubGlobal("innerWidth", 1024);
  vi.stubGlobal("innerHeight", 768);
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: vi.fn(() => hit) });
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
});

afterEach(() => { page.__jarvisGuard?.cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe("page snapshots and locators", () => {
  it("executes the serialized function without surrounding module references", () => {
    document.body.innerHTML = '<label for="email">E-mail</label><input id="email">';
    const execute = new Function("args", `return (${pageOperation.toString()})(args)`) as (args: PageOperation) => ReturnType<typeof pageOperation>;
    expect(execute({ action: "snapshot", generation: "serialized" })).toMatchObject({ elements: [
      expect.objectContaining({ id: "serialized-1", role: "textbox", name: "E-mail" }),
    ] });
    expect(execute({ action: "prepare", locator: { label: "E-mail" }, mode: "fill" })).toMatchObject({ ready: true, tag: "input" });
  });

  it("executes production-minified serialization without the generated module closure", () => {
    const source = readFileSync("browser-extension/page.ts", "utf8");
    // esbuild requires Node's native typed-array realm, outside jsdom globals.
    const code = execFileSync(process.execPath, ["--input-type=module", "-e", `
      import { transformWithEsbuild } from "vite";
      let source = "";
      for await (const chunk of process.stdin) source += chunk;
      const result = await transformWithEsbuild(source, "page.ts", { minify: true, target: "es2020", format: "iife", globalName: "SerializedPage" });
      process.stdout.write(result.code);
    `], { input: source, encoding: "utf8", timeout: 10000 });
    const bundled = new Function(`${code}; return SerializedPage.pageOperation;`)() as typeof pageOperation;
    const execute = new Function(`return (${bundled.toString()});`)() as typeof pageOperation;
    document.body.innerHTML = '<label for="email">E-mail</label><input id="email">';
    expect(execute({ action: "snapshot", generation: "minified" })).toMatchObject({ elements: [expect.objectContaining({ id: "minified-1", name: "E-mail" })] });
    const result = execute({ action: "prepare", element: "minified-1", mode: "fill" });
    expect(result.ready).toBe(true);
    expect(execute({ action: "verify", element: "token" in result ? result.token : undefined })).toEqual({ ready: true });
  });

  it("works on HTTP pages where crypto.randomUUID is unavailable", () => {
    vi.stubGlobal("crypto", {});
    document.body.innerHTML = "<input>";
    expect(pageOperation({ action: "snapshot" })).toMatchObject({ elements: [expect.objectContaining({ role: "textbox" })] });
    const values: PageOperation = { action: "prepare", locator: { role: "textbox" }, mode: "fill" };
    expect(pageOperation(values)).toMatchObject({ ready: true, token: expect.stringContaining("prepared-") });
  });

  it("exposes native roles, accessible names and control state", () => {
    document.body.innerHTML = `
      <label for="email"> E-mail   pessoal </label><input id="email" placeholder="Outro nome">
      <span id="save-label">Salvar</span><button aria-labelledby="save-label" aria-label="Ignorado">Ícone</button>
      <button aria-label="Fechar">X</button><input placeholder="Pesquisar">
      <button><img alt="Enviar"></button><label><input type="checkbox" checked> Aceitar</label>
      <button aria-disabled="true" aria-expanded="false">Detalhes</button>
      <div role="checkbox" aria-checked="mixed" tabindex="0" aria-label="Todas"></div>`;
    const elements = snapshot().elements;
    expect(elements).toEqual(expect.arrayContaining([
      expect.objectContaining({ role: "textbox", name: "E-mail pessoal" }),
      expect.objectContaining({ role: "button", name: "Salvar" }),
      expect.objectContaining({ role: "button", name: "Fechar" }),
      expect.objectContaining({ role: "textbox", name: "Pesquisar" }),
      expect.objectContaining({ role: "button", name: "Enviar" }),
      expect.objectContaining({ role: "checkbox", name: "Aceitar", checked: true }),
      expect.objectContaining({ name: "Detalhes", disabled: true, expanded: false }),
      expect.objectContaining({ name: "Todas", checked: "mixed" }),
    ]));
    expect(pageOperation({ action: "prepare", locator: { role: "textbox", label: "E-mail pessoal" }, mode: "fill" }).ready).toBe(true);
  });

  it("paginates all discovered controls and expires the preceding snapshot", () => {
    document.body.innerHTML = Array.from({ length: 180 }, (_, index) => `<button>Item ${index}</button>`).join("");
    const first = snapshot({ limit: 2 });
    expect(first).toMatchObject({ total: 180, offset: 0, limit: 2 });
    const next = snapshot({ generation: "next", offset: 150, limit: 40 });
    expect(next.elements).toHaveLength(30);
    expect(next.elements![0]).toMatchObject({ id: "next-151", name: "Item 150" });
    expect(prepared(first.elements![0].id)).toMatchObject({ ready: false, code: "browser_stale_element" });
  });

  it("includes open shadow controls and resolves names within their root", () => {
    const host = document.createElement("x-form");
    document.body.append(host);
    const shadow = host.attachShadow({ mode: "open" });
    shadow.innerHTML = '<span id="caption">Continuar</span><button aria-labelledby="caption">Ícone</button>';
    const button = shadow.querySelector("button")!;
    hit = host;
    Object.defineProperty(shadow, "elementFromPoint", { configurable: true, value: vi.fn(() => button) });
    const id = snapshot().elements![0].id;
    expect(snapshot().elements).toEqual([expect.objectContaining({ role: "button", name: "Continuar" })]);
    expect(prepared(id).code).toBe("browser_element_unstable");
    expect(prepared(id)).toMatchObject({ ready: true, x: 80, y: 36 });
  });

  it("rejects ambiguous locators and matches normalized text with optional exactness", () => {
    document.body.innerHTML = '<button data-testid="one"> Salvar   rascunho </button><button>Salvar</button>';
    expect(pageOperation({ action: "prepare", locator: { role: "button", name: "salvar", exact: false }, mode: "click" })).toMatchObject({ ready: false, code: "browser_ambiguous_element" });
    expect(pageOperation({ action: "prepare", locator: { role: "button", name: "Salvar" }, mode: "click" }).code).toBe("browser_element_unstable");
    expect(pageOperation({ action: "prepare", locator: { role: "button", name: "Salvar rascunho", exact: true }, mode: "click" }).code).toBe("browser_element_unstable");
    expect(pageOperation({ action: "prepare", locator: { testId: "one" }, mode: "click" }).code).toBe("browser_element_unstable");
    document.body.innerHTML = '<div><button><span>Continuar</span></button></div>';
    hit = document.querySelector("span");
    const values: PageOperation = { action: "prepare", locator: { text: "Continuar", exact: true }, mode: "click" };
    expect(pageOperation(values).code).toBe("browser_element_unstable");
    expect(pageOperation(values)).toMatchObject({ ready: true, tag: "span" });
  });

  it("freshly resolves only a unique semantic replacement of a detached element", () => {
    const { id } = target('<button aria-label="Salvar">Ícone</button>');
    document.body.innerHTML = '<button aria-label="Salvar">Ícone</button>';
    hit = document.querySelector("button");
    expect(prepared(id).code).toBe("browser_element_unstable");
    expect(prepared(id).ready).toBe(true);
    document.body.innerHTML = '<button aria-label="Salvar">Ícone</button><button aria-label="Salvar">Ícone</button>';
    expect(prepared(id)).toMatchObject({ ready: false, code: "browser_ambiguous_element" });
  });

  it("bounds traversal and refuses to claim uniqueness from a truncated tree", () => {
    document.body.innerHTML = '<button>Primeiro</button>' + '<div></div>'.repeat(5005) + '<button>Final</button>';
    expect(snapshot()).toMatchObject({ total: 1, truncated: true });
    expect(pageOperation({ action: "prepare", locator: { role: "button", name: "Primeiro" }, mode: "click" })).toMatchObject({ ready: false, code: "browser_snapshot_truncated" });
  });
});

describe("actionability preparation", () => {
  it("waits for unchanged geometry and verifies the actual pointer target", () => {
    const { element, id } = target("<button>Enviar</button>");
    expect(prepared(id).code).toBe("browser_element_unstable");
    boxes.set(element, box(40));
    expect(prepared(id).code).toBe("browser_element_unstable");
    const overlay = document.createElement("div");
    document.body.append(overlay);
    hit = overlay;
    expect(prepared(id).code).toBe("browser_element_obscured");
    hit = element;
    expect(prepared(id)).toMatchObject({ ready: true, x: 100, y: 36, viewportWidth: 1024, viewportHeight: 768 });
    expect(element.scrollIntoView).not.toHaveBeenCalled();
  });

  it("scrolls an offscreen control once without resetting stable geometry on every poll", () => {
    const { element, id } = target("<button>Enviar</button>");
    boxes.set(element, box(20, 900));
    const scroll = vi.mocked(element.scrollIntoView);
    expect(prepared(id).code).toBe("browser_element_outside_viewport");
    expect(prepared(id).code).toBe("browser_element_outside_viewport");
    expect(scroll).toHaveBeenCalledTimes(1);
    boxes.set(element, box());
    expect(prepared(id).code).toBe("browser_element_unstable");
    expect(prepared(id).ready).toBe(true);
    expect(scroll).toHaveBeenCalledTimes(1);
  });

  it("waits for visibility and inherited enabled state, including shadow ancestors", () => {
    const { element, id } = target("<button>Enviar</button>");
    const wrapper = document.createElement("section");
    document.body.append(wrapper);
    wrapper.append(element);
    wrapper.style.display = "none";
    expect(prepared(id).code).toBe("browser_element_hidden");
    wrapper.style.display = "block";
    wrapper.setAttribute("aria-disabled", "true");
    expect(prepared(id).code).toBe("browser_element_disabled");
    wrapper.removeAttribute("aria-disabled");
    element.setAttribute("disabled", "");
    expect(prepared(id).code).toBe("browser_element_disabled");
    element.removeAttribute("disabled");
    expect(prepared(id).code).toBe("browser_element_unstable");
    expect(prepared(id).ready).toBe(true);
    const host = document.createElement("x-button");
    document.body.append(host);
    host.attachShadow({ mode: "open" }).append(element);
    host.setAttribute("aria-disabled", "true");
    expect(prepared(id).code).toBe("browser_element_disabled");
  });

  it("follows rendered visibility for zero-size, transparent and overridden visibility", () => {
    const { element, id } = target('<button style="opacity:0">Enviar</button>');
    boxes.set(element, box(20, 20, 0));
    expect(prepared(id).code).toBe("browser_element_hidden");
    boxes.set(element, box());
    const wrapper = document.createElement("section");
    wrapper.style.visibility = "hidden";
    document.body.append(wrapper);
    wrapper.append(element);
    (element as HTMLElement).style.visibility = "visible";
    expect(prepared(id).code).toBe("browser_element_unstable");
    expect(prepared(id).ready).toBe(true);
  });

  it("rejects readonly and nontext inputs before focusing or changing their values", () => {
    for (const html of ['<input readonly value="Original">', '<input type="file">', '<input type="checkbox">', '<textarea aria-readonly="true">Original</textarea>']) {
      const { element, id } = target(html);
      const focus = vi.spyOn(element as HTMLElement, "focus");
      expect(prepared(id, "fill")).toMatchObject({ ready: false, code: "browser_element_not_editable" });
      expect(focus).not.toHaveBeenCalled();
    }
    const { element, id } = target('<input value="Original">');
    const focus = vi.spyOn(element as HTMLElement, "focus");
    expect(prepared(id, "fill")).toMatchObject({ ready: true, tag: "input" });
    expect(prepared(id, "fill").ready).toBe(true);
    expect(focus).toHaveBeenCalledTimes(1);
    expect((element as HTMLInputElement).value).toBe("Original");
  });

  it("focuses a press target only after stability, without requiring pointer hits", () => {
    const { element, id } = target("<button>Enviar</button>");
    const focus = vi.spyOn(element as HTMLElement, "focus");
    hit = null;
    expect(prepared(id, "press").code).toBe("browser_element_unstable");
    expect(focus).not.toHaveBeenCalled();
    expect(prepared(id, "press")).toMatchObject({ ready: true });
    expect(focus).toHaveBeenCalledTimes(1);
    expect(document.activeElement).toBe(element);
  });

  it("rechecks editability if a focus handler changes the field", () => {
    const { element, id } = target("<input>");
    element.addEventListener("focus", () => element.setAttribute("readonly", ""));
    expect(prepared(id, "fill")).toMatchObject({ ready: false, code: "browser_element_not_editable" });
  });

  it("verifies current focus on repeated preparation and immediately before dispatch", () => {
    const { element, id } = target("<input>");
    const token = prepared(id, "fill").token;
    expect(pageOperation({ action: "verify", element: token })).toEqual({ ready: true });
    const other = document.createElement("input");
    document.body.append(other);
    other.focus();
    expect(pageOperation({ action: "verify", element: token })).toMatchObject({ ready: false, code: "browser_focus_changed" });
    expect(document.activeElement).toBe(other);
    const retry = prepared(id, "fill");
    expect(retry.ready).toBe(true);
    expect(retry.token).not.toBe(token);
    other.focus();
    expect(prepared(id, "fill")).toMatchObject({ ready: false, code: "browser_focus_changed" });
    expect(document.activeElement).toBe(other);
    const current = prepared(id, "fill").token;
    expect(document.activeElement).toBe(element);
    element.setAttribute("readonly", "");
    expect(pageOperation({ action: "verify", element: current }).code).toBe("browser_element_not_editable");
    element.remove();
    expect(pageOperation({ action: "verify", element: current }).code).toBe("browser_stale_element");
  });
});

describe("native select and wait operations", () => {
  it("blocks a mismatched click after preparation and removes guards on finish", () => {
    const { element, id } = target("<button>Enviar</button>");
    prepared(id);
    const token = prepared(id).token;
    const other = document.createElement("button");
    document.body.append(other);
    const click = capturedClick();
    const handled = vi.fn();
    other.addEventListener("click", handled);
    expect(pageOperation({ action: "guard", element: token })).toEqual({ ready: true });
    const event = trustedClick(other);
    click()(event);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(handled).not.toHaveBeenCalled();
    expect(pageOperation({ action: "finish", element: token })).toEqual({ blocked: true });
    other.click();
    expect(handled).toHaveBeenCalledTimes(1);
    expect(element.isConnected).toBe(true);
  });

  it("keeps a trusted input block sticky while preserving page-created custom events", () => {
    const { element, id } = target("<button>Enviar</button>");
    prepared(id);
    const token = prepared(id).token;
    const click = capturedClick();
    pageOperation({ action: "guard", element: token });
    const mismatch = trustedClick(document.body);
    click()(mismatch);
    const matching = trustedClick(element);
    click()(matching);
    expect(matching.preventDefault).toHaveBeenCalledOnce();
    const synthetic = new MouseEvent("click", { bubbles: true, composed: true, cancelable: true });
    document.body.dispatchEvent(synthetic);
    expect(synthetic.defaultPrevented).toBe(false);
    expect(pageOperation({ action: "finish", element: token })).toEqual({ blocked: true });
  });

  it("allows a matching shadow click and expires disconnected guards conservatively", () => {
    vi.useFakeTimers();
    const host = document.createElement("x-button");
    document.body.append(host);
    const shadow = host.attachShadow({ mode: "open" });
    shadow.innerHTML = "<button><span>Enviar</span></button>";
    const button = shadow.querySelector("button")!;
    hit = host;
    Object.defineProperty(shadow, "elementFromPoint", { configurable: true, value: vi.fn(() => button) });
    const id = snapshot().elements![0].id;
    prepared(id);
    const token = prepared(id).token;
    const click = capturedClick();
    const handled = vi.fn();
    button.addEventListener("click", handled);
    expect(pageOperation({ action: "guard", element: token }).ready).toBe(true);
    const native = trustedClick(button, [button, shadow, host, document]);
    click()(native);
    expect(native.preventDefault).not.toHaveBeenCalled();
    shadow.querySelector("span")!.dispatchEvent(new MouseEvent("click", { bubbles: true, composed: true, cancelable: true }));
    expect(handled).toHaveBeenCalledTimes(1);
    expect(pageOperation({ action: "finish", element: token })).toEqual({ blocked: false });
    pageOperation({ action: "guard", element: token });
    vi.advanceTimersByTime(3000);
    expect(pageOperation({ action: "finish", element: token })).toEqual({ blocked: true });
    pageOperation({ action: "guard", element: token });
    button.remove();
    expect(pageOperation({ action: "finish", element: token })).toEqual({ blocked: true });
  });

  it("selects by value or label once and notifies input/change observers", () => {
    const { element, id } = target('<select><option value="a">A</option><option value="b">Segunda opção</option></select>');
    const events: string[] = [];
    element.addEventListener("input", event => events.push(event.type));
    element.addEventListener("change", event => events.push(event.type));
    const token = prepared(id, "fill").token;
    expect(pageOperation({ action: "select", element: token, text: "Segunda opção" })).toEqual({ ok: true });
    expect((element as HTMLSelectElement).value).toBe("b");
    expect(events).toEqual(["input", "change"]);
    expect(pageOperation({ action: "select", element: token, text: "a" })).toMatchObject({ ok: false, code: "browser_stale_element" });
    expect(pageOperation({ action: "select", element: prepared(id, "fill").token, text: "a" })).toEqual({ ok: true });
    expect((element as HTMLSelectElement).value).toBe("a");
  });

  it("never retargets a prepared select that detached before mutation", () => {
    const { element, id } = target('<select><option>A</option><option>B</option></select>');
    const token = prepared(id, "fill").token;
    element.replaceWith(element.cloneNode(true));
    expect(pageOperation({ action: "select", element: token, text: "B" })).toMatchObject({ ok: false, code: "browser_stale_element" });
    expect(document.querySelector("select")?.value).toBe("A");
  });

  it("rejects nonexistent, duplicate and disabled options without changing selection", () => {
    const { element, id } = target('<select><option value="a">A</option><option value="b">Mesmo</option><option value="c">Mesmo</option><option value="d" disabled>D</option></select>');
    for (const [text, code] of [["Missing", "browser_option_not_found"], ["Mesmo", "browser_ambiguous_option"], ["d", "browser_element_disabled"]]) {
      expect(pageOperation({ action: "select", element: prepared(id, "fill").token, text })).toMatchObject({ ok: false, code });
      expect((element as HTMLSelectElement).value).toBe("a");
    }
  });

  it("supports hidden/detached absence and polls attached/visible state", () => {
    const locator = { testId: "loading" };
    expect(pageOperation({ action: "wait", locator, state: "hidden" })).toEqual({ ready: true });
    expect(pageOperation({ action: "wait", locator, state: "detached" })).toEqual({ ready: true });
    expect(pageOperation({ action: "wait", locator, state: "attached" }).ready).toBe(false);
    document.body.innerHTML = '<div data-testid="loading" tabindex="0" style="display:none">Carregando</div>';
    expect(pageOperation({ action: "wait", locator, state: "attached" })).toEqual({ ready: true });
    expect(pageOperation({ action: "wait", locator, state: "visible" }).ready).toBe(false);
    expect(pageOperation({ action: "wait", locator, state: "hidden" })).toEqual({ ready: true });
    (document.body.firstElementChild as HTMLElement).style.display = "block";
    expect(pageOperation({ action: "wait", locator, state: "visible" })).toEqual({ ready: true });
    const id = snapshot().elements![0].id;
    document.body.innerHTML = "";
    expect(pageOperation({ action: "wait", element: id, state: "detached" })).toEqual({ ready: true });
  });

  it("waits for interactive/complete document readiness and reports inspection state", () => {
    const state = vi.spyOn(document, "readyState", "get");
    state.mockReturnValue("loading");
    expect(pageOperation({ action: "wait", state: "ready" })).toMatchObject({ ready: false, code: "browser_page_not_ready" });
    state.mockReturnValue("interactive");
    expect(pageOperation({ action: "wait", state: "ready" })).toEqual({ ready: true });
    expect(pageOperation({ action: "inspect" })).toMatchObject({ title: "Teste", readyState: "interactive" });
  });
});
