import { openUrl } from "@tauri-apps/plugin-opener";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { isTerminalLinkModifierPressed, openTerminalLink, resolveTerminalWebUrl } from "./terminal-links";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

function mouseEvent(modifiers: { ctrlKey?: boolean; metaKey?: boolean } = {}) {
  return {
    ctrlKey: modifiers.ctrlKey ?? false,
    metaKey: modifiers.metaKey ?? false,
    preventDefault: vi.fn(),
  } as unknown as MouseEvent;
}

describe("terminal links", () => {
  beforeEach(() => vi.mocked(openUrl).mockReset().mockResolvedValue(undefined));

  it("uses Command on macOS and Control on Windows and Linux", () => {
    expect(isTerminalLinkModifierPressed(mouseEvent({ metaKey: true }), "MacIntel")).toBe(true);
    expect(isTerminalLinkModifierPressed(mouseEvent({ ctrlKey: true }), "MacIntel")).toBe(false);
    expect(isTerminalLinkModifierPressed(mouseEvent({ ctrlKey: true }), "Win32")).toBe(true);
    expect(isTerminalLinkModifierPressed(mouseEvent({ metaKey: true }), "Linux x86_64")).toBe(false);
  });

  it("opens a terminal web URL only with the platform modifier", async () => {
    const regularClick = mouseEvent();
    const commandClick = mouseEvent({ metaKey: true });

    await expect(openTerminalLink(regularClick, "http://localhost:3001", "MacIntel")).resolves.toBe(false);
    expect(openUrl).not.toHaveBeenCalled();
    expect(regularClick.preventDefault).not.toHaveBeenCalled();

    await expect(openTerminalLink(commandClick, "http://localhost:3001", "MacIntel")).resolves.toBe(true);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith("http://localhost:3001/");
    expect(commandClick.preventDefault).toHaveBeenCalledOnce();
  });

  it("rejects invalid URLs and protocols that should not leave the app", async () => {
    expect(resolveTerminalWebUrl("https://127.0.0.1:5173/path?q=1")).toBe("https://127.0.0.1:5173/path?q=1");
    expect(resolveTerminalWebUrl("file:///tmp/private.txt")).toBeNull();
    expect(resolveTerminalWebUrl("javascript:alert(1)")).toBeNull();
    expect(resolveTerminalWebUrl("not a url")).toBeNull();

    const click = mouseEvent({ ctrlKey: true });
    await expect(openTerminalLink(click, "file:///tmp/private.txt", "Win32")).resolves.toBe(false);
    expect(openUrl).not.toHaveBeenCalled();
    expect(click.preventDefault).not.toHaveBeenCalled();
  });
});
