import { z } from "zod";
import { address, bounded, BrowserError, CommandQueue, nativeTabId, object, tabHandle, validateMethod, type Request } from "./protocol";
import { pageOperation, type PageOperation } from "./page";
import { frameOwnerOperation } from "./frames";

type Owner = { conversationId: string; created: boolean };
type Log = { level: string; text: string; time: number };
type NetworkEntry = { id: string; method: string; url: string; status?: number; type?: string; failed?: string };
type Frame = { id: string; parentId?: string; sessionId?: string; url: string; name?: string; available?: boolean };
type FrameOwner = { objectId: string; sessionId?: string };
const ownerSchema = z.record(z.string(), z.object({ conversationId: z.string(), created: z.boolean() }));
const MAX_TABS = 12;
const BUFFER_LIMIT = 200;
const BODY_LIMIT = 64000;

export class BrowserController {
  private owners = new Map<number, Owner>();
  private active = new Map<string, number>();
  private attached = new Set<number>();
  private detached = new Set<number>();
  private contexts = new Map<number, Map<string, number>>();
  private elements = new Map<number, Map<string, Frame>>();
  private sessions = new Map<number, Map<string, Frame>>();
  private logs = new Map<number, Log[]>();
  private network = new Map<number, NetworkEntry[]>();
  private queue = new CommandQueue();
  private storage = new CommandQueue();
  private validity = new Map<number, () => boolean>();
  private generation = 0;

  constructor(readonly epoch: string, private changed: (conversationId: string) => void) {}

  async restore(): Promise<void> {
    const saved = await chrome.storage.session.get(["owners", "active"]);
    const owners = ownerSchema.safeParse(saved.owners);
    const live = new Set((await chrome.tabs.query({})).map(tab => tab.id));
    if (owners.success) for (const [id, owner] of Object.entries(owners.data)) {
      if (live.has(Number(id))) this.owners.set(Number(id), owner);
    }
    for (const [conversation, id] of Object.entries(object(saved.active))) {
      if (typeof id === "number" && this.owners.get(id)?.conversationId === conversation) this.active.set(conversation, id);
    }
    // The target's attached flag also includes DevTools and other extensions.
    // A harmless command verifies our ownership without taking their debugger.
    for (const target of await chrome.debugger.getTargets()) {
      if (target.tabId !== undefined && target.attached && this.owners.has(target.tabId)) {
        try { await this.cdp(target.tabId, "Runtime.enable"); this.attached.add(target.tabId); }
        catch { this.detached.add(target.tabId); }
      }
    }
    await this.persist();
  }

  private persist() {
    return this.storage.run(0, () => chrome.storage.session.set({ owners: Object.fromEntries(this.owners), active: Object.fromEntries(this.active) }));
  }

  async releaseAll(): Promise<void> {
    this.generation++;
    // Fence pending creates immediately, then drain the creation queue before
    // releasing per-tab ownership. A late tabs.create result stays unowned.
    await this.queue.run(-1, async () => {
      for (const id of this.owners.keys()) await this.queue.run(id, () => this.release(id));
    });
  }

  private async release(id: number): Promise<void> {
    const owner = this.owners.get(id);
    this.owners.delete(id);
    this.attached.delete(id);
    this.detached.delete(id);
    this.contexts.delete(id);
    this.elements.delete(id);
    this.sessions.delete(id);
    this.logs.delete(id);
    this.network.delete(id);
    if (owner && this.active.get(owner.conversationId) === id) this.active.delete(owner.conversationId);
    await chrome.debugger.detach({ tabId: id }).catch(() => {});
    await this.persist();
    if (owner) this.changed(owner.conversationId);
  }

  removed(id: number): void { void this.release(id).catch(() => {}); }

  updated(id: number, change: chrome.tabs.OnUpdatedInfo): void {
    const owner = this.owners.get(id);
    if (!owner) return;
    if (change.status === "loading" || change.url) { this.contexts.delete(id); this.elements.delete(id); }
    this.changed(owner.conversationId);
  }

  onDetach(id: number): void {
    if (!this.owners.has(id)) return;
    this.attached.delete(id);
    this.contexts.delete(id);
    this.elements.delete(id);
    this.sessions.delete(id);
    this.detached.add(id);
    const owner = this.owners.get(id);
    if (owner) this.changed(owner.conversationId);
  }

  event(id: number, method: string, raw: unknown, sessionId?: string): void {
    if (!this.owners.has(id)) return;
    const params = object(raw);
    if (method === "Target.attachedToTarget") {
      const info = object(params.targetInfo);
      if (info.type === "iframe" && typeof params.sessionId === "string" && typeof info.targetId === "string") {
        const sessions = this.sessions.get(id) ?? new Map<string, Frame>();
        sessions.set(params.sessionId, { id: info.targetId, sessionId: params.sessionId, url: String(info.url ?? "") });
        this.sessions.set(id, sessions);
        void this.enableFrameSession(id, params.sessionId).catch(() => {});
      }
      return;
    }
    if (method === "Target.detachedFromTarget" && typeof params.sessionId === "string") {
      this.sessions.get(id)?.delete(params.sessionId);
      this.contexts.delete(id); this.elements.delete(id);
      return;
    }
    if (method === "Runtime.executionContextsCleared" || method === "Page.frameNavigated" || method === "Page.navigatedWithinDocument") { this.contexts.delete(id); this.elements.delete(id); }
    if (method === "Runtime.consoleAPICalled" || method === "Runtime.exceptionThrown" || method === "Log.entryAdded") {
      const entry = object(params.entry);
      const exception = object(params.exceptionDetails);
      const args = Array.isArray(params.args) ? params.args.map(item => {
        const arg = object(item);
        return typeof arg.value === "string" ? arg.value : JSON.stringify(arg.value ?? arg.description ?? arg.type ?? "");
      }).join(" ") : "";
      const text = String(args || entry.text || object(exception.exception).description || exception.text || "").slice(0, 3000);
      const list = this.logs.get(id) ?? [];
      list.push({ level: String(params.type || entry.level || "error"), text, time: Date.now() });
      this.logs.set(id, list.slice(-BUFFER_LIMIT));
    }
    if (method === "Network.requestWillBeSent") {
      const request = object(params.request);
      const list = this.network.get(id) ?? [];
      const requestId = String(params.requestId ?? "");
      const handle = sessionId ? `${sessionId}:${requestId}` : requestId;
      const previous = list.findIndex(item => item.id === handle);
      if (previous >= 0) list.splice(previous, 1);
      list.push({ id: handle, url: String(request.url ?? "").slice(0, 4096), method: String(request.method ?? ""), type: String(params.type ?? "") });
      this.network.set(id, list.slice(-BUFFER_LIMIT));
    }
    if (method === "Network.responseReceived" || method === "Network.loadingFailed") {
      const item = this.network.get(id)?.find(item => item.id === (sessionId ? `${sessionId}:${String(params.requestId)}` : params.requestId));
      if (item) {
        const response = object(params.response);
        if (typeof response.status === "number") item.status = response.status;
        if (params.errorText) item.failed = String(params.errorText).slice(0, 500);
      }
    }
  }

  private own(id: number, conversationId: string): Owner {
    const owner = this.owners.get(id);
    if (!owner || owner.conversationId !== conversationId) throw new BrowserError("browser_tab_not_owned", "A aba não pertence a esta conversa. Selecione uma aba disponível para conectá-la.");
    return owner;
  }

  private async attach(id: number): Promise<void> {
    if (this.attached.has(id)) return;
    try { await chrome.debugger.attach({ tabId: id }, "1.3"); }
    catch { throw new BrowserError("browser_debugger_unavailable", "Não foi possível conectar à aba. Feche o DevTools ou outro depurador e conecte a aba novamente."); }
    this.attached.add(id);
    this.detached.delete(id);
    try {
      for (const method of ["Runtime.enable", "Page.enable", "Log.enable"]) await this.cdp(id, method);
      await this.cdp(id, "Network.enable", { maxTotalBufferSize: 4_000_000, maxResourceBufferSize: 1_000_000 });
      await this.cdp(id, "Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true, filter: [{ type: "iframe", exclude: false }, { exclude: true }] });
    } catch (error) { this.attached.delete(id); await chrome.debugger.detach({ tabId: id }).catch(() => {}); throw error; }
  }

  private async enableFrameSession(id: number, sessionId: string): Promise<void> {
    for (const method of ["Runtime.enable", "Page.enable", "Log.enable", "Network.enable"]) await this.cdp(id, method, {}, sessionId);
    await this.cdp(id, "Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true, filter: [{ type: "iframe", exclude: false }, { exclude: true }] }, sessionId);
  }

  private async cdp(id: number, method: string, params: Record<string, unknown> = {}, sessionId?: string, timeoutMs = 25000): Promise<Record<string, unknown>> {
    this.requireConnection(this.validity.get(id));
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      const result = await Promise.race([
        chrome.debugger.sendCommand({ tabId: id, ...(sessionId ? { sessionId } : {}) }, method, params),
        new Promise<never>((_, reject) => {
          timeout = setTimeout(() => reject(new BrowserError("browser_outcome_unknown", "O navegador não confirmou o resultado a tempo. Inspecione a página antes de repetir uma ação.")), timeoutMs);
        }),
      ]);
      return object(result);
    } catch (error) {
      if (sessionId && error instanceof Error && /session|target closed|frame.*not found/i.test(error.message)) {
        this.sessions.get(id)?.delete(sessionId);
        this.contexts.delete(id); this.elements.delete(id);
        throw new BrowserError("browser_frame_detached", "O frame foi substituído. Capture um novo snapshot antes de continuar.");
      }
      if (error instanceof Error && /not attached|No tab with given id|target closed/i.test(error.message)) {
        this.onDetach(id);
        throw new BrowserError("browser_debugger_detached", "O depurador não está conectado a esta aba. Feche o DevTools e conecte a aba novamente com browser_attach.");
      }
      throw error;
    } finally { clearTimeout(timeout); }
  }

  private async evaluate(id: number, expression: string, frame?: Frame): Promise<unknown> {
    let contextId = frame ? this.contexts.get(id)?.get(frame.id) : undefined;
    if (frame && contextId === undefined) {
      const world = await this.cdp(id, "Page.createIsolatedWorld", { frameId: frame.id, worldName: "jarvis-browser" }, frame.sessionId);
      if (typeof world.executionContextId !== "number") throw new BrowserError("browser_context_unavailable", "A página ainda não está pronta para inspeção.");
      contextId = world.executionContextId;
      const contexts = this.contexts.get(id) ?? new Map<string, number>();
      contexts.set(frame.id, contextId);
      this.contexts.set(id, contexts);
    }
    const result = await this.cdp(id, "Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true, timeout: 20000, ...(frame ? { contextId } : {}) }, frame?.sessionId);
    if (result.exceptionDetails) {
      const detail = object(result.exceptionDetails);
      throw new BrowserError("browser_evaluation_failed", String(object(detail.exception).description || detail.text || "Falha ao executar JavaScript.").slice(0, 1000));
    }
    return object(result.result).value ?? object(result.result).description ?? null;
  }

  private async frames(id: number): Promise<Frame[]> {
    // Child debugger sessions are needed for out-of-process cross-origin frames.
    // This also restores frame discovery after extension worker suspension.
    await this.cdp(id, "Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true, filter: [{ type: "iframe", exclude: false }, { exclude: true }] });
    const sessions = [...(this.sessions.get(id)?.values() ?? [])].slice(0, 32);
    const trees = await Promise.all([undefined, ...sessions.map(frame => frame.sessionId)].map(async sessionId => {
      try { return { sessionId, tree: await this.cdp(id, "Page.getFrameTree", {}, sessionId) }; }
      catch (cause) { if (!sessionId) throw cause; return { sessionId, tree: {} }; }
    }));
    const frames = new Map<string, Frame>();
    const visit = (raw: unknown, sessionId?: string, parentId?: string, depth = 0) => {
      if (depth > 16 || frames.size >= 100) return;
      const tree = object(raw), data = object(tree.frame);
      if (typeof data.id !== "string") return;
      const previous = frames.get(data.id);
      frames.set(data.id, { id: data.id, url: String(data.url ?? ""), name: String(data.name ?? ""),
        parentId: typeof data.parentId === "string" ? data.parentId : parentId ?? previous?.parentId,
        sessionId: sessionId ?? sessions.find(frame => frame.id === data.id)?.sessionId,
        available: true });
      if (Array.isArray(tree.childFrames)) for (const child of tree.childFrames) visit(child, sessionId, data.id, depth + 1);
    };
    for (const { sessionId, tree } of trees) visit(tree.frameTree, sessionId);
    for (const session of sessions) if (!trees.find(tree => tree.sessionId === session.sessionId)?.tree.frameTree) {
      frames.set(session.id, { ...session, parentId: frames.get(session.id)?.parentId, available: false });
    }
    if (!frames.size) throw new BrowserError("browser_context_unavailable", "A página ainda não informou seus frames. Aguarde ou capture um novo snapshot.");
    return [...frames.values()];
  }

  private selectFrame(frames: Frame[], frameId?: string | null): Frame {
    const frame = frameId ? frames.find(frame => frame.id === frameId) : frames.find(frame => !frame.parentId && !frame.sessionId);
    if (!frame || frame.available === false) throw new BrowserError("browser_frame_detached", "O frame não está disponível. Capture um novo snapshot para selecionar o frame atual.");
    if (frame.url && !/^(https?:|about:blank|about:srcdoc)/.test(frame.url)) throw new BrowserError("browser_frame_unavailable", "Este frame não pertence a uma página web disponível para interação.");
    return frame;
  }

  private page(id: number, frame: Frame, args: PageOperation) {
    return this.evaluate(id, `(${pageOperation.toString()})(${JSON.stringify(args)})`, frame).then(object);
  }

  private async ownerOperation(id: number, owner: FrameOwner, action: "point" | "guard" | "finish", point: Record<string, unknown> = {}) {
    const result = await this.cdp(id, "Runtime.callFunctionOn", { objectId: owner.objectId,
      functionDeclaration: frameOwnerOperation.toString(), arguments: [{ value: { action, x: point.x, y: point.y } }], returnByValue: true }, owner.sessionId);
    if (result.exceptionDetails) throw new BrowserError("browser_frame_unavailable", "Não foi possível verificar a posição do frame. Capture um novo snapshot.");
    return object(object(result.result).value);
  }

  private async releaseOwners(id: number, owners: FrameOwner[]): Promise<void> {
    await Promise.all(owners.map(owner => this.cdp(id, "Runtime.releaseObject", { objectId: owner.objectId }, owner.sessionId).catch(() => {})));
  }

  private async pointerPoint(id: number, frame: Frame, frames: Frame[], initial: Record<string, unknown>) {
    let current = frame, point = initial;
    const owners: FrameOwner[] = [];
    let retained = false;
    try {
      for (let depth = 0; current.parentId; depth++) {
        if (depth >= 16) throw new BrowserError("browser_frame_unavailable", "A profundidade de frames excedeu o limite de inspeção.");
        const parent = this.selectFrame(frames, current.parentId);
        // Resolve in the parent's isolated world: owner rectangles remain LOCAL
        // to each parent viewport, avoiding double offsets for in-process frames.
        await this.page(id, parent, { action: "inspect" });
        const node = await this.cdp(id, "DOM.getFrameOwner", { frameId: current.id }, parent.sessionId);
        const resolved = await this.cdp(id, "DOM.resolveNode", { backendNodeId: node.backendNodeId,
          executionContextId: this.contexts.get(id)?.get(parent.id) }, parent.sessionId);
        const objectId = object(resolved.object).objectId;
        if (typeof objectId !== "string") throw new BrowserError("browser_frame_unavailable", "O navegador não informou o elemento do frame.");
        const owner = { objectId, sessionId: parent.sessionId };
        owners.push(owner);
        point = await this.ownerOperation(id, owner, "point", point);
        if (point.ready !== true) return { ...point, owners: [] as FrameOwner[] };
        current = parent;
      }
      retained = true;
      return { ...initial, x: point.x, y: point.y, owners };
    } finally { if (!retained) await this.releaseOwners(id, owners); }
  }

  private async waitFor(id: number, request: Request["request"], operation: () => Promise<Record<string, unknown>>) {
    const start = Date.now(), timeout = request.timeoutMs ?? 5000;
    const deadline = start + timeout;
    let last: Record<string, unknown> = {};
    for (let attempt = 0; attempt < 160; attempt++) {
      this.requireConnection(this.validity.get(id));
      last = await operation();
      this.requireConnection(this.validity.get(id));
      if (last.ready === true) return last;
      const code = String(last.code ?? "browser_action_timeout");
      if (["browser_ambiguous_element", "browser_stale_element", "browser_element_not_editable", "browser_element_not_focusable", "browser_hit_test_unavailable", "browser_frame_transform_unsupported", "browser_focus_changed", "browser_snapshot_truncated", "browser_invalid_request"].includes(code)) {
        throw new BrowserError(code, String(last.reason ?? "O alvo não está disponível para esta ação."), { action: request.action, phase: "prepare", dispatched: false });
      }
      if (Date.now() >= deadline) break;
      await new Promise(resolve => setTimeout(resolve, Math.min(100, deadline - Date.now())));
    }
    throw new BrowserError("browser_action_timeout", `A condição não foi atendida: ${String(last.reason ?? "página ainda não pronta")}. Nenhum input foi enviado; inspecione a página ou ajuste o alvo.`,
      { action: request.action, phase: "prepare", dispatched: false, elapsedMs: Date.now() - start, reason: last.code });
  }

  private async tab(id: number, owner: Owner) {
    const tab = await chrome.tabs.get(id);
    return { id: tabHandle(this.epoch, id), conversationId: owner.conversationId, title: tab.title || "Nova aba", url: tab.url || "about:blank", loading: tab.status === "loading" };
  }

  private async snapshot(conversationId: string) {
    const tabs = [];
    for (const [id, owner] of this.owners) if (owner.conversationId === conversationId) {
      try { tabs.push(await this.tab(id, owner)); } catch { await this.release(id); }
    }
    const active = this.active.get(conversationId);
    return { tabs, activeId: active === undefined ? tabs[0]?.id ?? null : tabHandle(this.epoch, active), backend: "extension" };
  }

  private requireConnection(valid?: () => boolean): void {
    if (valid && !valid()) throw new BrowserError("browser_outcome_unknown", "A conexão foi interrompida. Inspecione a página antes de repetir uma ação; etapas pendentes foram descartadas.");
  }

  private requireElement(id: number, element?: string | null): Frame {
    const frame = element ? this.elements.get(id)?.get(element) : undefined;
    if (!frame) throw new BrowserError("browser_stale_element", "O elemento não pertence ao snapshot atual. Capture um novo snapshot antes de agir.");
    return frame;
  }

  private async interact(id: number, request: Request["request"]): Promise<unknown> {
    if ((request.action === "fill" && request.text == null) || request.action === "press" && !request.key) throw new BrowserError("browser_invalid_request", "Informe o texto ou a tecla da ação.");
    const keys: Record<string, { code: string; value: number; text?: string }> = { Enter: { code: "Enter", value: 13, text: "\r" }, Tab: { code: "Tab", value: 9 }, Escape: { code: "Escape", value: 27 }, ArrowLeft: { code: "ArrowLeft", value: 37 }, ArrowUp: { code: "ArrowUp", value: 38 }, ArrowRight: { code: "ArrowRight", value: 39 }, ArrowDown: { code: "ArrowDown", value: 40 }, Backspace: { code: "Backspace", value: 8 }, Delete: { code: "Delete", value: 46 }, Space: { code: "Space", value: 32, text: " " } };
    const key = keys[request.key ?? ""];
    if (request.action === "press" && !key) throw new BrowserError("browser_invalid_request", "Tecla não suportada.");
    const frames = await this.frames(id);
    const elementFrame = request.element ? this.requireElement(id, request.element) : undefined;
    if (elementFrame && request.frameId && request.frameId !== elementFrame.id) throw new BrowserError("browser_invalid_request", "O elemento não pertence ao frame selecionado.");
    const frame = this.selectFrame(frames, elementFrame?.id ?? request.frameId);
    const start = Date.now();
    let owners: FrameOwner[] = [], dispatched = false, guarded = false, token: string | undefined;
    try {
      const prepared = await this.waitFor(id, request, async () => {
        await this.releaseOwners(id, owners); owners = [];
        if (request.element) this.requireElement(id, request.element);
        const ready = await this.page(id, frame, { action: "prepare", mode: request.action as "click" | "fill" | "press", element: request.element ?? undefined, locator: request.locator ?? undefined });
        if (ready.ready !== true || request.action !== "click") return ready;
        const point = await this.pointerPoint(id, frame, frames, ready);
        owners = point.owners;
        return point;
      });
      token = typeof prepared.token === "string" ? prepared.token : undefined;
      if (request.action === "click") {
        if (!token || !Number.isFinite(prepared.x) || !Number.isFinite(prepared.y)) throw new BrowserError("browser_invalid_result", "O navegador não confirmou um alvo de clique válido.");
        let guardDeadline = Infinity;
        for (const owner of owners) {
          guardDeadline = Math.min(guardDeadline, Date.now() + 3000);
          const result = await this.ownerOperation(id, owner, "guard");
          if (result.ready !== true) throw new BrowserError(String(result.code ?? "browser_frame_unavailable"), String(result.reason ?? "O frame mudou antes do clique."));
        }
        // Install the page guard last and conservatively include RPC latency in
        // each guard's lifetime. A slow setup must never dispatch after expiry.
        guardDeadline = Math.min(guardDeadline, Date.now() + 3000);
        const guard = await this.page(id, frame, { action: "guard", element: token });
        if (guard.ready !== true) throw new BrowserError(String(guard.code ?? "browser_stale_element"), String(guard.reason ?? "O alvo mudou antes do clique."));
        guarded = true;
        if (Date.now() >= guardDeadline) throw new BrowserError("browser_guard_unavailable", "A preparação do clique excedeu a validade da proteção. Nenhum input foi enviado.", { action: "click", phase: "prepare", dispatched: false });
        // Once ANY input is sent, failures must not restart the action.
        this.requireConnection(this.validity.get(id)); dispatched = true;
        await this.cdp(id, "Input.dispatchMouseEvent", { type: "mousePressed", x: prepared.x, y: prepared.y, button: "left", clickCount: 1 }, undefined, Math.max(1, guardDeadline - Date.now()));
        await this.cdp(id, "Input.dispatchMouseEvent", { type: "mouseReleased", x: prepared.x, y: prepared.y, button: "left", clickCount: 1 }, undefined, Math.max(1, guardDeadline - Date.now()));
        const result = await this.page(id, frame, { action: "finish", element: token });
        guarded = false;
        if (result.blocked === true || result.code) throw new BrowserError("browser_outcome_unknown", "O alvo mudou durante o clique. Inspecione a página antes de repetir uma ação.");
        for (const owner of owners) {
          const result = await this.ownerOperation(id, owner, "finish");
          if (result.blocked === true || result.ready !== true) throw new BrowserError("browser_outcome_unknown", "O frame mudou durante o clique. Inspecione a página antes de repetir uma ação.");
        }
      } else if (request.action === "fill") {
        const verified = await this.page(id, frame, { action: "verify", element: token });
        if (verified.ready !== true) throw new BrowserError(String(verified.code ?? "browser_stale_element"), String(verified.reason ?? "O campo mudou antes da edição."));
        this.requireConnection(this.validity.get(id)); dispatched = true;
        if (prepared.tag === "select") {
          const result = await this.page(id, frame, { action: "select", element: token, text: request.text ?? "" });
          if (result.ok !== true) throw new BrowserError(String(result.code ?? "browser_element_not_editable"), String(result.reason ?? "Não foi possível selecionar a opção."));
        } else {
          await this.cdp(id, "Input.dispatchKeyEvent", { type: "keyDown", key: "a", code: "KeyA", windowsVirtualKeyCode: 65, modifiers: 2, commands: ["selectAll"] });
          await this.cdp(id, "Input.dispatchKeyEvent", { type: "keyUp", key: "a", code: "KeyA", windowsVirtualKeyCode: 65, modifiers: 2 });
          const focused = await this.page(id, frame, { action: "verify", element: token });
          if (focused.ready !== true) throw new BrowserError("browser_focus_changed", "O foco mudou durante a seleção do texto; a edição não foi repetida.");
          if (request.text) await this.cdp(id, "Input.insertText", { text: request.text });
          else {
            await this.cdp(id, "Input.dispatchKeyEvent", { type: "keyDown", key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8 });
            await this.cdp(id, "Input.dispatchKeyEvent", { type: "keyUp", key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8 });
          }
        }
      } else {
        const verified = await this.page(id, frame, { action: "verify", element: token });
        if (verified.ready !== true) throw new BrowserError(String(verified.code ?? "browser_stale_element"), String(verified.reason ?? "O foco mudou antes da tecla."));
        this.requireConnection(this.validity.get(id)); dispatched = true;
        await this.cdp(id, "Input.dispatchKeyEvent", { type: "keyDown", key: request.key, code: key.code, windowsVirtualKeyCode: key.value, ...(key.text ? { text: key.text } : {}) });
        await this.cdp(id, "Input.dispatchKeyEvent", { type: "keyUp", key: request.key, code: key.code, windowsVirtualKeyCode: key.value });
      }
      const page = await this.tab(id, this.owners.get(id)!).catch(() => null);
      return { ok: true, dispatched: true, frameId: frame.id, elapsedMs: Date.now() - start, page,
        instructions: "Input was dispatched once. This does not prove the site's action succeeded. Confirm the expected outcome with browser_wait or snapshot; never blindly repeat an action." };
    } catch (cause) {
      if (dispatched) throw new BrowserError("browser_outcome_unknown", "Uma ação foi enviada, mas seu resultado não foi confirmado. Inspecione a página antes de repetir.",
        { action: request.action, phase: "dispatch", dispatched: true, elapsedMs: Date.now() - start, reason: cause instanceof Error ? cause.message.slice(0, 500) : "unknown" });
      throw cause;
    } finally {
      if (guarded && token) await this.page(id, frame, { action: "finish", element: token }).catch(() => {});
      for (const owner of owners) await this.ownerOperation(id, owner, "finish").catch(() => {});
      await this.releaseOwners(id, owners);
    }
  }

  async execute(message: Request, connected: () => boolean = () => true): Promise<unknown> {
    const generation = this.generation;
    const valid = () => generation === this.generation && connected();
    const { conversationId, request } = message;
    if (!conversationId && request.action !== "prune") throw new BrowserError("browser_invalid_request", "A conversa é obrigatória.");
    const id = request.id ? nativeTabId(this.epoch, request.id) : undefined;
    return this.queue.run(id ?? -1, async () => {
      this.requireConnection(valid);
      let guardedId = id;
      if (guardedId !== undefined) this.validity.set(guardedId, valid);
      try {
      if (request.action === "prune") {
        const retained = new Set(request.retained ?? []);
        for (const [tabId, owner] of this.owners) if (!retained.has(owner.conversationId)) await this.queue.run(tabId, () => this.release(tabId));
        return { released: true };
      }
      if (request.action === "list") return this.snapshot(conversationId);
      if (request.action === "discover") {
        const tabs = (await chrome.tabs.query({})).filter(tab => {
          try { return tab.id !== undefined && Boolean(address(tab.url ?? "")); } catch { return false; }
        }).slice(0, 100).map(tab => ({ id: tabHandle(this.epoch, tab.id!), title: tab.title || "Sem título", url: tab.url || "", owned: this.owners.has(tab.id!) }));
        return { tabs };
      }
      if (request.action === "open" || request.action === "attach") {
        if ((id === undefined || !this.owners.has(id)) && ([...this.owners.values()].filter(owner => owner.conversationId === conversationId).length >= MAX_TABS || this.owners.size >= 100)) throw new BrowserError("browser_tab_limit", "Feche ou desconecte uma aba antes de abrir outra (limite de 12 por conversa).");
        let tabId = id;
        if (request.action === "open") {
          const url = request.url ? address(request.url) : "about:blank";
          const created = request.newWindow ? (await chrome.windows.create({ url, focused: true }))?.tabs?.[0] : await chrome.tabs.create({ url, active: true });
          tabId = created?.id;
        } else {
          if (tabId === undefined) throw new BrowserError("browser_invalid_request", "Selecione uma aba para conectar.");
          const existing = this.owners.get(tabId);
          if (existing && existing.conversationId !== conversationId) throw new BrowserError("browser_tab_busy", "Esta aba está sendo usada por outra conversa.");
          const url = (await chrome.tabs.get(tabId)).url ?? "";
          if (url !== "about:blank" || !existing?.created) address(url);
        }
        if (tabId === undefined) throw new BrowserError("browser_open_failed", "O navegador não informou a aba criada.");
        this.requireConnection(valid);
        guardedId = tabId;
        this.validity.set(tabId, valid);
        // Ownership is saved before attaching, so an interrupted attach never loses
        // the tab and an operation with unknown outcome is not replayed.
        this.owners.set(tabId, { conversationId, created: request.action === "open" || this.owners.get(tabId)?.created === true });
        this.active.set(conversationId, tabId);
        await this.persist();
        this.requireConnection(valid);
        await this.attach(tabId);
        this.changed(conversationId);
        return this.snapshot(conversationId);
      }
      if (id === undefined) throw new BrowserError("browser_invalid_request", "A operação exige uma aba.");
      const owner = this.own(id, conversationId);
      if (request.action === "close") {
        await chrome.tabs.remove(id);
        await this.release(id);
        return this.snapshot(conversationId);
      }
      if (request.action === "select") {
        const tab = await chrome.tabs.update(id, { active: true });
        if (!tab) throw new BrowserError("browser_stale_tab", "A aba foi fechada antes de receber o foco.");
        this.requireConnection(valid);
        await chrome.windows.update(tab.windowId, { focused: true });
        this.active.set(conversationId, id);
        await this.persist();
        this.changed(conversationId);
        return this.snapshot(conversationId);
      }
      if (request.action === "navigate") {
        if (!request.url) throw new BrowserError("browser_invalid_request", "Informe a URL.");
        this.contexts.delete(id);
        this.elements.delete(id);
        await chrome.tabs.update(id, { url: address(request.url) });
        return this.tab(id, owner);
      }
      const tab = await chrome.tabs.get(id);
      if (tab.url !== "about:blank" || !owner.created) address(tab.url ?? "");
      if (this.detached.has(id)) throw new BrowserError("browser_debugger_detached", "O depurador foi desconectado (por exemplo, ao abrir o DevTools). Feche o DevTools e conecte esta aba novamente com browser_attach.");
      await this.attach(id);
      this.requireConnection(valid);
      switch (request.action) {
        case "back": case "forward": {
          this.contexts.delete(id);
          this.elements.delete(id);
          if (request.action === "back") await chrome.tabs.goBack(id); else await chrome.tabs.goForward(id);
          return { ok: true };
        }
        case "reload": this.contexts.delete(id); this.elements.delete(id); await chrome.tabs.reload(id); return { ok: true };
        case "snapshot": {
          const frames = await this.frames(id), frame = this.selectFrame(frames, request.frameId);
          const result = await this.page(id, frame, { action: "snapshot", generation: crypto.randomUUID().slice(0, 8), offset: request.offset ?? 0, limit: request.limit ?? 100 });
          const elements = result.elements;
          this.elements.set(id, new Map(Array.isArray(elements) ? elements.flatMap(item => typeof object(item).id === "string" ? [[String(object(item).id), frame] as const] : []) : []));
          return { ...result, frameId: frame.id, frames: frames.map(frame => ({ id: frame.id, parentId: frame.parentId, url: frame.url, name: frame.name, available: frame.available })) };
        }
        case "wait": {
          const frames = await this.frames(id);
          const elementFrame = request.element ? this.requireElement(id, request.element) : undefined;
          if (elementFrame && request.frameId && request.frameId !== elementFrame.id) throw new BrowserError("browser_invalid_request", "O elemento não pertence ao frame selecionado.");
          const frame = this.selectFrame(frames, elementFrame?.id ?? request.frameId);
          const start = Date.now();
          await this.waitFor(id, request, () => {
            if (request.element) this.requireElement(id, request.element);
            return this.page(id, frame, { action: "wait", element: request.element ?? undefined, locator: request.locator ?? undefined, state: request.state ?? "ready" });
          });
          return { ready: true, state: request.state ?? "ready", frameId: frame.id, elapsedMs: Date.now() - start };
        }
        case "console": return { logs: this.logs.get(id) ?? [] };
        case "network": {
          const list = (this.network.get(id) ?? []).filter(item => !request.filter || `${item.method} ${item.url} ${item.status ?? ""}`.toLowerCase().includes(request.filter.toLowerCase()));
          const offset = request.offset ?? 0, limit = request.limit ?? 30;
          return { requests: list.slice(offset, offset + limit), total: list.length, offset, limit };
        }
        case "response_body": {
          if (!request.requestId || !this.network.get(id)?.some(item => item.id === request.requestId)) throw new BrowserError("browser_request_expired", "Esta requisição não está no buffer da aba. A captura começa ao conectar e mantém as últimas 200 requisições.");
          const split = request.requestId.indexOf(":");
          const sessionId = split >= 0 ? request.requestId.slice(0, split) : undefined;
          const body = await this.cdp(id, "Network.getResponseBody", { requestId: split >= 0 ? request.requestId.slice(split + 1) : request.requestId }, sessionId);
          const text = String(body.body ?? "");
          return { body: text.slice(0, BODY_LIMIT), base64Encoded: body.base64Encoded === true, truncated: text.length > BODY_LIMIT, totalCharacters: text.length };
        }
        case "screenshot": {
          const image = await this.cdp(id, "Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
          if (typeof image.data !== "string" || image.data.length > 20_000_000) throw new BrowserError("browser_screenshot_too_large", "A captura excedeu o limite de tamanho.");
          return { data: image.data, url: tab.url ?? "" };
        }
        case "evaluate": {
          if (!request.expression) throw new BrowserError("browser_invalid_request", "Informe o JavaScript a executar.");
          return bounded(await this.evaluate(id, request.expression));
        }
        case "devtools": {
          const method = request.method ?? "", params = request.params ?? {};
          validateMethod(method, params);
          if (method === "Page.navigate") { this.contexts.delete(id); this.elements.delete(id); }
          return bounded(await this.cdp(id, method, params));
        }
        case "click": case "fill": case "press": return await this.interact(id, request);
        case "scroll": {
          await this.cdp(id, "Input.dispatchMouseEvent", { type: "mouseWheel", x: 1, y: 1, deltaX: request.x ?? 0, deltaY: request.y ?? 600 });
          return { ok: true };
        }
      }
      } finally {
        if (guardedId !== undefined && this.validity.get(guardedId) === valid) this.validity.delete(guardedId);
      }
    });
  }
}
