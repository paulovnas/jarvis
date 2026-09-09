import { afterEach, describe, expect, it } from "vitest";
import { installWebviewShortcutGuards, preventWebviewReload } from "./webview-shortcuts";

afterEach(() => document.removeEventListener("keydown", preventWebviewReload, true));

describe("WebView shortcut guards", () => {
  it.each([
    { key: "F5" },
    { key: "r", metaKey: true },
    { key: "R", ctrlKey: true, shiftKey: true },
  ])("blocks the reload shortcut $key", (init) => {
    const event = new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true });
    preventWebviewReload(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it("keeps application and editing shortcuts available", () => {
    const event = new KeyboardEvent("keydown", { key: "s", metaKey: true, bubbles: true, cancelable: true });
    preventWebviewReload(event);
    expect(event.defaultPrevented).toBe(false);
  });

  it("installs the guard before events reach the app", () => {
    installWebviewShortcutGuards();
    const event = new KeyboardEvent("keydown", { key: "F5", bubbles: true, cancelable: true });
    document.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });
});
