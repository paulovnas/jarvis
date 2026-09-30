import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { frameOwnerOperation } from "./frames";

let owners: HTMLIFrameElement[];
const page = window as unknown as { __jarvisFrameGuards?: WeakMap<HTMLElement, unknown> };
const frame = (rect = new DOMRect(100, 50, 300, 200)) => {
  const owner = document.createElement("iframe");
  owner.style.border = "0";
  owner.style.padding = "0";
  document.body.append(owner);
  vi.spyOn(owner, "getBoundingClientRect").mockReturnValue(rect);
  vi.spyOn(document, "elementFromPoint").mockReturnValue(owner);
  owners.push(owner);
  return owner;
};
const capturedClick = (type = "click") => {
  const events = vi.spyOn(document, "addEventListener");
  return () => {
    const listener = events.mock.calls.find(([eventType]) => eventType === type)?.[1];
    if (typeof listener !== "function") throw new Error("No click guard was installed.");
    return listener;
  };
};
const trustedClick = (target: Element) => ({
  isTrusted: true, clientX: 125, clientY: 75, composedPath: () => [target, document],
  preventDefault: vi.fn(), stopPropagation: vi.fn(), stopImmediatePropagation: vi.fn(),
} as unknown as MouseEvent);

beforeEach(() => {
  vi.useFakeTimers();
  owners = [];
  document.body.innerHTML = "";
  delete page.__jarvisFrameGuards;
});

afterEach(() => {
  for (const owner of owners) frameOwnerOperation.call(owner, { action: "finish" });
  vi.restoreAllMocks();
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe("frame owner viewport coordinates", () => {
  it("adds fractional borders and padding once without scrolling a visible frame", () => {
    const owner = frame(new DOMRect(100.25, 50.75, 300, 200));
    owner.style.borderLeft = "1.5px solid black";
    owner.style.borderTop = "2.25px solid black";
    owner.style.paddingLeft = "3.75px";
    owner.style.paddingTop = "4.5px";
    const scroll = vi.spyOn(owner, "scrollIntoView");
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20.5, y: 30.25 })).toEqual({ ready: true, x: 126, y: 87.75 });
    expect(document.elementFromPoint).toHaveBeenCalledWith(126, 87.75);
    expect(scroll).not.toHaveBeenCalled();
  });

  it("scrolls only when the action point is outside the parent viewport", () => {
    const owner = frame(new DOMRect(-100, 50, 300, 200));
    const bounds = vi.mocked(owner.getBoundingClientRect);
    vi.spyOn(owner, "scrollIntoView").mockImplementation(() => { bounds.mockReturnValue(new DOMRect(10, 20, 300, 200)); });
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toEqual({ ready: true, x: 30, y: 50 });
    expect(owner.scrollIntoView).toHaveBeenCalledOnce();
  });

  it("does not approve coordinates outside the frame content or parent viewport", () => {
    const owner = frame();
    owner.style.paddingRight = "10px";
    expect(frameOwnerOperation.call(owner, { action: "point", x: 295, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_point_outside" });
    vi.mocked(owner.getBoundingClientRect).mockReturnValue(new DOMRect(2000, 50, 300, 200));
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_point_outside" });
    for (const x of [-1, NaN, Infinity]) expect(frameOwnerOperation.call(owner, { action: "point", x, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_point_invalid" });
  });

  it("rejects detached, hidden and zero-size frame owners", () => {
    const owner = frame();
    owner.style.display = "none";
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_not_visible" });
    owner.style.display = "";
    vi.mocked(owner.getBoundingClientRect).mockReturnValue(new DOMRect());
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_not_visible" });
    owner.remove();
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_detached" });
  });

  it.each(["transform: scale(1.2)", "rotate: 15deg", "scale: 2", "translate: 10px", "zoom: 125%", "perspective: 500px"])("refuses unsupported owner ancestor geometry: %s", style => {
    const owner = frame();
    const parent = document.createElement("div");
    parent.style.cssText = style;
    document.body.append(parent);
    parent.append(owner);
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_transform_unsupported" });
  });

  it("rejects an overlay at the owner's parent point", () => {
    const owner = frame();
    vi.mocked(document.elementFromPoint).mockReturnValue(document.createElement("div"));
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_obstructed" });
  });

  it("checks nested shadow roots and their outer hosts for obstruction", () => {
    const owner = frame();
    const outerHost = document.createElement("div");
    const outerRoot = outerHost.attachShadow({ mode: "open" });
    const innerHost = document.createElement("div");
    const innerRoot = innerHost.attachShadow({ mode: "open" });
    document.body.append(outerHost);
    outerRoot.append(innerHost);
    innerRoot.append(owner);
    const outerHit = vi.fn().mockReturnValue(innerHost);
    Object.defineProperty(outerRoot, "elementFromPoint", { value: outerHit });
    Object.defineProperty(innerRoot, "elementFromPoint", { value: vi.fn().mockReturnValue(owner) });
    vi.mocked(document.elementFromPoint).mockReturnValue(outerHost);
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toEqual({ ready: true, x: 120, y: 80 });
    outerHit.mockReturnValue(document.createElement("div"));
    expect(frameOwnerOperation.call(owner, { action: "point", x: 20, y: 30 })).toMatchObject({ ready: false, code: "browser_frame_obstructed" });
  });

  it("runs after serialization without module closures", () => {
    const owner = frame();
    const serialized = new Function(`return (${frameOwnerOperation.toString()});`)() as typeof frameOwnerOperation;
    expect(serialized.call(owner, { action: "point", x: 20, y: 30 })).toEqual({ ready: true, x: 120, y: 80 });
  });
});

describe("frame input guard", () => {
  it.each(["click", "pointerover", "mouseover"])("blocks trusted mismatched %s input and reports the block when finishing", type => {
    const owner = frame();
    const click = capturedClick(type);
    expect(frameOwnerOperation.call(owner, { action: "guard" })).toEqual({ ready: true });
    const event = trustedClick(document.body);
    click()(event);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopImmediatePropagation).toHaveBeenCalledOnce();
    expect(frameOwnerOperation.call(owner, { action: "finish" })).toEqual({ ready: true, blocked: true });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("allows trusted owner input and leaves page-created events alone", () => {
    const owner = frame();
    const click = capturedClick();
    frameOwnerOperation.call(owner, { action: "guard" });
    const event = trustedClick(owner);
    click()(event);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(document.body.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }))).toBe(true);
    expect(frameOwnerOperation.call(owner, { action: "finish" })).toEqual({ ready: true, blocked: false });
  });

  it("removes document and shadow-root guards after completion or connection-loss timeout", () => {
    const owner = frame();
    const host = document.createElement("div");
    const root = host.attachShadow({ mode: "open" });
    document.body.append(host);
    root.append(owner);
    const documentAdd = vi.spyOn(document, "addEventListener");
    const documentRemove = vi.spyOn(document, "removeEventListener");
    const rootAdd = vi.spyOn(root, "addEventListener");
    const rootRemove = vi.spyOn(root, "removeEventListener");
    frameOwnerOperation.call(owner, { action: "guard" });
    vi.advanceTimersByTime(3000);
    for (const [type, listener] of documentAdd.mock.calls) expect(documentRemove).toHaveBeenCalledWith(type, listener, true);
    for (const [type, listener] of rootAdd.mock.calls) expect(rootRemove).toHaveBeenCalledWith(type, listener, true);
    expect(frameOwnerOperation.call(owner, { action: "finish" })).toMatchObject({ ready: false, code: "browser_outcome_unknown", blocked: false });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps separate frame guards and replaces an existing owner guard", () => {
    const first = frame();
    const second = frame();
    frameOwnerOperation.call(first, { action: "guard" });
    frameOwnerOperation.call(second, { action: "guard" });
    frameOwnerOperation.call(first, { action: "guard" });
    expect(vi.getTimerCount()).toBe(2);
    expect(frameOwnerOperation.call(first, { action: "finish" })).toEqual({ ready: true, blocked: false });
    expect(vi.getTimerCount()).toBe(1);
    expect(frameOwnerOperation.call(second, { action: "finish" })).toEqual({ ready: true, blocked: false });
    expect(vi.getTimerCount()).toBe(0);
  });
});
