import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { writeText as writeNativeClipboardText } from "@tauri-apps/plugin-clipboard-manager";
import { nativeClipboardAvailable, writeClipboardText } from "./clipboard";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn() }));

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

  it("reports an unavailable browser clipboard instead of claiming success", async () => {
    Reflect.deleteProperty(navigator, "clipboard");
    await expect(writeClipboardText("texto")).rejects.toThrow("Clipboard API unavailable");
  });
});
