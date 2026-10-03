type Prepared = { token?: string; element: Element; mode?: string };
type Action = { action: "click" | "fill" | "press"; token: string; text?: string; key?: string };

// Serialized into the extension's isolated world, sharing pageOperation's state.
export function firefoxInput(args: Action): Record<string, unknown> {
  const page = window as unknown as { __jarvisPrepared?: Prepared };
  const prepared = page.__jarvisPrepared;
  if (!prepared || prepared.token !== args.token || !prepared.element.isConnected) return { ok: false, code: "browser_stale_element", dispatched: false };
  const element = prepared.element;
  if (!(element instanceof HTMLElement)) return { ok: false, code: "browser_element_not_focusable", dispatched: false };
  // Validate in the same script turn that dispatches, avoiding a target swap
  // between the background's preparation RPC and DOM activation.
  const rejected = (code: string) => { page.__jarvisPrepared = undefined; return { ok: false, code, dispatched: false }; };
  if (element.matches(":disabled") || element.getAttribute("aria-disabled") === "true" || element.closest("[inert]")) return rejected("browser_element_disabled");
  const rect = element.getBoundingClientRect();
  const style = getComputedStyle(element);
  if (rect.width <= 0 || rect.height <= 0 || style.display === "none" || style.visibility === "hidden") return rejected("browser_element_hidden");
  let frameX = (Math.max(0, rect.left) + Math.min(innerWidth, rect.right)) / 2;
  let frameY = (Math.max(0, rect.top) + Math.min(innerHeight, rect.bottom)) / 2;
  // frameElement is inaccessible across origins. Do not bypass the parent's
  // visibility or overlays by activating a child from an isolated world.
  for (let current: Window = window; current !== current.parent;) {
    let owner: Element | null;
    try { owner = current.frameElement; } catch { return rejected("browser_frame_input_unsupported"); }
    if (!owner) return rejected("browser_frame_input_unsupported");
    const parent = owner.ownerDocument.defaultView;
    const box = owner.getBoundingClientRect();
    if (!parent || box.width <= 0 || box.height <= 0) return rejected("browser_frame_not_visible");
    for (let ancestor: Element | null = owner; ancestor;) {
      const ownStyle = parent.getComputedStyle(ancestor);
      if (ownStyle.visibility === "hidden" || ownStyle.display === "none" || ownStyle.contentVisibility === "hidden") return rejected("browser_frame_not_visible");
      if (ownStyle.transform && ownStyle.transform !== "none" || ownStyle.translate && ownStyle.translate !== "none" || ownStyle.scale && ownStyle.scale !== "none" || ownStyle.rotate && ownStyle.rotate !== "none" || ownStyle.zoom && !["normal", "1", "100%"].includes(ownStyle.zoom)) return rejected("browser_frame_input_unsupported");
      const root = ancestor.getRootNode();
      ancestor = ancestor.parentElement ?? (root instanceof parent.ShadowRoot ? root.host : null);
    }
    const ownerStyle = parent.getComputedStyle(owner);
    frameX += box.left + (parseFloat(ownerStyle.borderLeftWidth) || 0) + (parseFloat(ownerStyle.paddingLeft) || 0);
    frameY += box.top + (parseFloat(ownerStyle.borderTopWidth) || 0) + (parseFloat(ownerStyle.paddingTop) || 0);
    if (args.action === "click") {
      let hit = owner.ownerDocument.elementFromPoint(frameX, frameY);
      while (hit?.shadowRoot) { const inner = hit.shadowRoot.elementFromPoint(frameX, frameY); if (!inner || inner === hit) break; hit = inner; }
      if (hit !== owner) return rejected("browser_frame_obstructed");
    }
    current = parent;
  }
  if (args.action === "click") {
    const x = (Math.max(0, rect.left) + Math.min(innerWidth, rect.right)) / 2;
    const y = (Math.max(0, rect.top) + Math.min(innerHeight, rect.bottom)) / 2;
    let hit = document.elementFromPoint(x, y);
    while (hit?.shadowRoot) { const inner = hit.shadowRoot.elementFromPoint(x, y); if (!inner || inner === hit) break; hit = inner; }
    let target: Element | null = hit;
    while (target && target !== element) target = target.parentElement ?? (target.getRootNode() instanceof ShadowRoot ? (target.getRootNode() as ShadowRoot).host : null);
    if (!target) return rejected("browser_element_obscured");
  } else {
    let focused = document.activeElement;
    while (focused?.shadowRoot?.activeElement) focused = focused.shadowRoot.activeElement;
    if (focused !== element) return rejected("browser_focus_changed");
    if (args.action === "fill" && (element.hasAttribute("readonly") || element.getAttribute("aria-readonly") === "true")) return rejected("browser_element_not_editable");
  }
  page.__jarvisPrepared = undefined;
  // Firefox WebExtensions cannot dispatch trusted browser input. DOM actions
  // preserve ordinary default activation; sites requiring isTrusted may reject it.
  if (args.action === "click") {
    const point = { clientX: (Math.max(0, rect.left) + Math.min(innerWidth, rect.right)) / 2, clientY: (Math.max(0, rect.top) + Math.min(innerHeight, rect.bottom)) / 2,
      button: 0, bubbles: true, cancelable: true, composed: true };
    const mouseCompatibility = typeof PointerEvent !== "function" || element.dispatchEvent(new PointerEvent("pointerdown", { ...point, buttons: 1, pointerId: 1, pointerType: "mouse", isPrimary: true }));
    if (!element.isConnected) return { ok: false, code: "browser_outcome_unknown", dispatched: true };
    if (mouseCompatibility && element.dispatchEvent(new MouseEvent("mousedown", { ...point, buttons: 1 }))) element.focus({ preventScroll: true });
    if (!element.isConnected) return { ok: false, code: "browser_outcome_unknown", dispatched: true };
    if (typeof PointerEvent === "function") element.dispatchEvent(new PointerEvent("pointerup", { ...point, buttons: 0, pointerId: 1, pointerType: "mouse", isPrimary: true }));
    if (mouseCompatibility) element.dispatchEvent(new MouseEvent("mouseup", { ...point, buttons: 0 }));
    if (!element.isConnected) return { ok: false, code: "browser_outcome_unknown", dispatched: true };
    element.click(); return { ok: true, dispatched: true, trustedInput: false };
  }
  if (args.action === "fill") {
    const value = args.text ?? "";
    if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
      const prototype = element instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
      if (!setter) return { ok: false, code: "browser_element_not_editable", dispatched: false };
      setter.call(element, value);
    } else if (element.isContentEditable) { element.textContent = value; }
    else return { ok: false, code: "browser_element_not_editable", dispatched: false };
    element.dispatchEvent(new InputEvent("input", { bubbles: true, composed: true, inputType: "insertText", data: value }));
    element.dispatchEvent(new Event("change", { bubbles: true, composed: true }));
    return { ok: true, dispatched: true, trustedInput: false };
  }
  const key = args.key ?? "";
  if (!["Enter", "Tab", "Escape", "ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown", "Backspace", "Delete", "Space"].includes(key)) return { ok: false, code: "browser_invalid_request", dispatched: false };
  // Browser default actions are not performed for synthetic keyboard events.
  // Implement only safe ordinary activation; report the remaining keys honestly.
  if (key === "Tab" || key === "Backspace" || key === "Delete") return { ok: false, code: "browser_unsupported_input", dispatched: false };
  const eventKey = key === "Space" ? " " : key;
  const allowed = element.dispatchEvent(new KeyboardEvent("keydown", { key: eventKey, code: key, bubbles: true, cancelable: true, composed: true }));
  if (allowed && (key === "Enter" || key === "Space")) {
    if (element.matches("button,a[href],input[type=button],input[type=submit],input[type=reset],input[type=checkbox],input[type=radio],summary")) element.click();
    else if (key === "Enter" && element instanceof HTMLInputElement && element.form) element.form.requestSubmit();
  }
  element.dispatchEvent(new KeyboardEvent("keyup", { key: eventKey, code: key, bubbles: true, composed: true }));
  return { ok: true, dispatched: true, trustedInput: false, nativeDefaultActions: false };
}

export type FirefoxLog = { level: string; text: string; time: number };

// Runs in MAIN, so all returned console data remains untrusted page content.
export function firefoxConsole(action: "start" | "read" | "stop"): { logs: FirefoxLog[] } {
  type State = { logs: FirefoxLog[]; stop: () => void };
  const page = window as unknown as { __jarvisFirefoxConsole?: State };
  if (action === "stop") { page.__jarvisFirefoxConsole?.stop(); delete page.__jarvisFirefoxConsole; return { logs: [] }; }
  if (action === "start" && !page.__jarvisFirefoxConsole) {
    const logs: FirefoxLog[] = [];
    const push = (level: string, values: unknown[]) => {
      const text = values.map(value => {
        try { return typeof value === "string" ? value : value instanceof Error ? value.message : JSON.stringify(value); }
        catch { return "[unserializable]"; }
      }).join(" ").slice(0, 3000);
      logs.push({ level, text, time: Date.now() });
      if (logs.length > 200) logs.shift();
    };
    const methods = ["log", "info", "warn", "error", "debug"] as const;
    const original = new Map<string, (...values: unknown[]) => void>();
    const wrapped = new Map<string, (...values: unknown[]) => void>();
    for (const level of methods) {
      const previous = console[level];
      const replacement = (...values: unknown[]) => { push(level, values); previous.apply(console, values); };
      original.set(level, previous); wrapped.set(level, replacement); console[level] = replacement;
    }
    const error = (event: ErrorEvent) => push("error", [event.message]);
    const rejection = (event: PromiseRejectionEvent) => push("error", [event.reason]);
    window.addEventListener("error", error); window.addEventListener("unhandledrejection", rejection);
    page.__jarvisFirefoxConsole = { logs, stop: () => {
      for (const level of methods) if (console[level] === wrapped.get(level)) console[level] = original.get(level)!;
      window.removeEventListener("error", error); window.removeEventListener("unhandledrejection", rejection);
    } };
  }
  return { logs: Array.isArray(page.__jarvisFirefoxConsole?.logs) ? page.__jarvisFirefoxConsole.logs.slice(-200).map(item => ({
    level: String(item?.level ?? "log").slice(0, 100), text: String(item?.text ?? "").slice(0, 3000), time: Number.isFinite(item?.time) ? item.time : 0,
  })) : [] };
}

export async function firefoxEvaluate(expression: string): Promise<unknown> {
  // MAIN respects the site's CSP. CSP refusal is surfaced rather than bypassed.
  // Explicit page-scoped evaluation; extension secrets are never injected.
  return await (0, eval)(expression);
}

export function firefoxScroll(point: { x: number; y: number }): { ok: true } {
  window.scrollBy({ left: point.x, top: point.y, behavior: "instant" });
  return { ok: true };
}
