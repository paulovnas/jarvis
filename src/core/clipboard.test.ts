import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readText as readNativeClipboardText, writeText as writeNativeClipboardText } from "@tauri-apps/plugin-clipboard-manager";
import { nativeClipboardAvailable, readClipboardText, writeClipboardText } from "./clipboard";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ readText: vi.fn(), writeText: vi.fn() }));

const originalTauriInternals = Object.getOwnPropertyDescriptor(window, "__TAURI_INTERNALS__");
const originalClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
const browserWriteText = vi.fn<(text: string) => Promise<void>>();

function setNativeRuntime(enabled: boolean) {
  if (enabled) Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
  else Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
}

describe("clipboard", () => {
  beforeEach(() => {
    vi.mocked(writeNativeClipboardText).mockReset().mockResolvedValue(undefined);
    browserWriteText.mockReset().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: browserWriteText } });
    setNativeRuntime(false);
  });

  afterEach(() => {
    if (originalTauriInternals) Object.defineProperty(window, "__TAURI_INTERNALS__", originalTauriInternals);
    else Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    if (originalClipboard) Object.defineProperty(navigator, "clipboard", originalClipboard);
    else Reflect.deleteProperty(navigator, "clipboard");
  });

  it("writes through the native Tauri clipboard inside the desktop app", async () => {
    setNativeRuntime(true);
    expect(nativeClipboardAvailable()).toBe(true);

    await writeClipboardText("php artisan migrate");

    expect(writeNativeClipboardText).toHaveBeenCalledExactlyOnceWith("php artisan migrate");
    expect(browserWriteText).not.toHaveBeenCalled();
  });

  it("keeps browser previews usable outside Tauri", async () => {
    expect(nativeClipboardAvailable()).toBe(false);

    await writeClipboardText("bun run dev");

    expect(browserWriteText).toHaveBeenCalledExactlyOnceWith("bun run dev");
    expect(writeNativeClipboardText).not.toHaveBeenCalled();
  });

  it("reads pasted text through the native clipboard only in the desktop app", async () => {
    setNativeRuntime(true);
    vi.mocked(readNativeClipboardText).mockResolvedValueOnce("texto nativo");
    await expect(readClipboardText()).resolves.toBe("texto nativo");
    expect(readNativeClipboardText).toHaveBeenCalledOnce();
    setNativeRuntime(false);
    const readText = vi.fn().mockResolvedValue("texto web");
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { readText } });
    await expect(readClipboardText()).resolves.toBe("texto web");
    expect(readText).toHaveBeenCalledOnce();
  });

  it("reports an unavailable browser clipboard instead of claiming success", async () => {
    Reflect.deleteProperty(navigator, "clipboard");
    await expect(writeClipboardText("texto")).rejects.toThrow("Clipboard API unavailable");
  });
});
