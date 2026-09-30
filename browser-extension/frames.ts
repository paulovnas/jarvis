export type FrameOwnerArgs = { action: "point" | "guard" | "finish"; x?: number; y?: number };
export type FrameOwnerResult = { ready: boolean; code?: string; reason?: string; x?: number; y?: number; blocked?: boolean };

// Serialized into the parent's isolated world: no surrounding runtime dependencies.
export function frameOwnerOperation(this: HTMLElement, args: FrameOwnerArgs): FrameOwnerResult {
  type Guard = { blocked: boolean; expired: boolean; stop: () => void };
  // eslint-disable-next-line @typescript-eslint/no-this-alias -- DOM traversal cursors start at the serialized CDP receiver.
  const owner = this;
  const document = this.ownerDocument;
  const view = document.defaultView;
  const pending = (code: string, reason: string): FrameOwnerResult => ({ ready: false, code, reason });
  if (!view) return pending("browser_frame_detached", "The frame's parent document is unavailable.");
  const page = view as unknown as { __jarvisFrameGuards?: WeakMap<HTMLElement, Guard> };
  const previous = page.__jarvisFrameGuards?.get(this);
  if (args.action === "finish") {
    previous?.stop();
    page.__jarvisFrameGuards?.delete(this);
    return previous?.expired
      ? { ...pending("browser_outcome_unknown", "The frame input guard expired. Inspect the page before repeating an action."), blocked: previous.blocked }
      : { ready: true, blocked: previous?.blocked ?? false };
  }
  if (!this.isConnected) return pending("browser_frame_detached", "The frame was detached. Request a new snapshot.");
  const parent = (element: Element): Element | null => {
    const root = element.getRootNode();
    return element.assignedSlot ?? element.parentElement ?? (root instanceof view.ShadowRoot ? root.host : null);
  };
  for (let element: Element | null = owner; element; element = parent(element)) {
    const style = view.getComputedStyle(element);
    if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse" || style.contentVisibility === "hidden") {
      return pending("browser_frame_not_visible", "The frame is hidden.");
    }
    if (style.transform && style.transform !== "none"
      || style.translate && style.translate !== "none"
      || style.rotate && style.rotate !== "none" && !/^0(?:deg|rad|turn)?$/.test(style.rotate)
      || style.scale && style.scale !== "none" && !/^1(?:\s+1){0,2}$/.test(style.scale)
      || style.zoom && !["normal", "1", "100%"].includes(style.zoom)
      || style.perspective && style.perspective !== "none") {
      return pending("browser_frame_transform_unsupported", "Transformed or zoomed frames require direct CDP inspection; no input was sent.");
    }
  }
  const hitOwner = (x: number, y: number) => {
    let target: Element = owner;
    while (true) {
      const root = target.getRootNode();
      if (!(root instanceof view.ShadowRoot)) return root === document && typeof document.elementFromPoint === "function" && document.elementFromPoint(x, y) === target;
      if (typeof root.elementFromPoint !== "function" || root.elementFromPoint(x, y) !== target) return false;
      target = root.host;
    }
  };
  if (args.action === "guard") {
    previous?.stop();
    const guard: Guard = { blocked: false, expired: false, stop: () => {} };
    const roots: (Document | ShadowRoot)[] = [document];
    for (let element: Element | null = owner; element;) {
      const root = element.getRootNode();
      if (!(root instanceof view.ShadowRoot)) break;
      roots.push(root);
      element = root.host;
    }
    const events = ["pointerover", "pointermove", "pointerdown", "pointerup", "mouseover", "mousemove", "mousedown", "mouseup", "click", "auxclick", "dblclick", "contextmenu"];
    const listener = (event: Event) => {
      if (!event.isTrusted) return;
      const point = event as MouseEvent;
      if (!guard.blocked && event.composedPath().includes(this) && hitOwner(point.clientX, point.clientY)) return;
      guard.blocked = true;
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
    };
    for (const root of roots) for (const event of events) root.addEventListener(event, listener, { capture: true, passive: false });
    const timer = view.setTimeout(() => { guard.expired = true; guard.stop(); }, 3000);
    guard.stop = () => {
      view.clearTimeout(timer);
      for (const root of roots) for (const event of events) root.removeEventListener(event, listener, true);
    };
    (page.__jarvisFrameGuards ??= new WeakMap()).set(this, guard);
    return { ready: true };
  }
  if (args.action !== "point" || !Number.isFinite(args.x) || !Number.isFinite(args.y) || args.x! < 0 || args.y! < 0) {
    return pending("browser_frame_point_invalid", "Frame coordinates must be finite nonnegative numbers.");
  }
  const style = view.getComputedStyle(this);
  const pixels = (value: string) => parseFloat(value) || 0;
  const left = pixels(style.borderLeftWidth) + pixels(style.paddingLeft);
  const top = pixels(style.borderTopWidth) + pixels(style.paddingTop);
  let rect = this.getBoundingClientRect();
  if (!rect.width || !rect.height) return pending("browser_frame_not_visible", "The frame has no visible dimensions.");
  if (rect.left + left + args.x! < 0 || rect.top + top + args.y! < 0
    || rect.left + left + args.x! >= view.innerWidth || rect.top + top + args.y! >= view.innerHeight) {
    this.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
    rect = this.getBoundingClientRect();
  }
  const width = rect.width - left - pixels(style.borderRightWidth) - pixels(style.paddingRight);
  const height = rect.height - top - pixels(style.borderBottomWidth) - pixels(style.paddingBottom);
  const x = rect.left + left + args.x!;
  const y = rect.top + top + args.y!;
  if (width <= 0 || height <= 0 || args.x! >= width || args.y! >= height || x < 0 || y < 0 || x >= view.innerWidth || y >= view.innerHeight) {
    return pending("browser_frame_point_outside", "The action point is outside the visible frame viewport.");
  }
  if (!hitOwner(x, y)) return pending("browser_frame_obstructed", "Another element intercepts the frame's pointer events.");
  return { ready: true, x, y };
}
