import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readText as readNativeClipboardText, writeText as writeNativeClipboardText, writeImage as writeNativeClipboardImage } from "@tauri-apps/plugin-clipboard-manager";
import { Image as NativeImage } from "@tauri-apps/api/image";
import { nativeClipboardAvailable, readClipboardText, writeClipboardText, writeClipboardImage } from "./clipboard";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ readText: vi.fn(), writeText: vi.fn(), writeImage: vi.fn() }));

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
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
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

  function imageMocks() {
    const decode = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("Image", class {
      src = "";
      naturalWidth = 2;
      naturalHeight = 1;
      decode = decode;
    });
    const data = new Uint8ClampedArray([255, 0, 0, 255, 0, 255, 0, 0]);
    const context = { drawImage: vi.fn(), getImageData: vi.fn().mockReturnValue({ data }) };
    // Only the two canvas methods used for copying pixels are needed in jsdom.
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
    return { decode, context, data };
  }

  it("writes image pixels and transparency to the native clipboard and releases its resource", async () => {
    setNativeRuntime(true);
    const { context, data } = imageMocks();
    const image = new NativeImage(7);
    const close = vi.spyOn(image, "close").mockResolvedValue(undefined);
    const create = vi.spyOn(NativeImage, "new").mockResolvedValue(image);
    vi.mocked(writeNativeClipboardImage).mockReset().mockResolvedValue(undefined);

    await writeClipboardImage("data:image/png;base64,cGljdHVyZQ==");

    expect(context.getImageData).toHaveBeenCalledWith(0, 0, 2, 1);
    expect(create).toHaveBeenCalledExactlyOnceWith(new Uint8Array(data), 2, 1);
    expect(writeNativeClipboardImage).toHaveBeenCalledExactlyOnceWith(image);
    expect(close).toHaveBeenCalledOnce();
    expect(writeNativeClipboardText).not.toHaveBeenCalled();
  });

  it("releases the image and reports a rejected native copy without writing text", async () => {
    setNativeRuntime(true);
    imageMocks();
    const image = new NativeImage(8);
    const close = vi.spyOn(image, "close").mockResolvedValue(undefined);
    vi.spyOn(NativeImage, "new").mockResolvedValue(image);
    vi.mocked(writeNativeClipboardImage).mockReset().mockRejectedValue(new Error("Clipboard locked"));

    await expect(writeClipboardImage("data:image/jpeg;base64,cGljdHVyZQ==")).rejects.toThrow("Clipboard locked");

    expect(close).toHaveBeenCalledOnce();
    expect(writeNativeClipboardText).not.toHaveBeenCalled();
  });

  it("starts the browser clipboard write before decoding and supplies a PNG rather than a URL", async () => {
    const { decode } = imageMocks();
    let finishDecode: () => void = () => {};
    decode.mockReturnValue(new Promise<void>(resolve => { finishDecode = resolve; }));
    const blob = new Blob(["pixels"], { type: "image/png" });
    vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation(callback => callback(blob));
    vi.stubGlobal("ClipboardItem", class {
      types = ["image/png"];
      constructor(private data: Record<string, Promise<Blob>>) {}
      getType(type: string) { return this.data[type]; }
    });
    const write = vi.fn<(items: ClipboardItem[]) => Promise<void>>().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { write } });

    const copying = writeClipboardImage("data:image/webp;base64,cGljdHVyZQ==");

    expect(write).toHaveBeenCalledOnce();
    expect(write.mock.calls[0][0][0].types).toEqual(["image/png"]);
    finishDecode();
    await copying;
    await expect(write.mock.calls[0][0][0].getType("image/png")).resolves.toBe(blob);
    expect(browserWriteText).not.toHaveBeenCalled();
  });

  it("reports unsupported image clipboard and decoding failures", async () => {
    await expect(writeClipboardImage("data:image/png;base64,bad")).rejects.toThrow("Clipboard API unavailable");
    setNativeRuntime(true);
    const { decode } = imageMocks();
    decode.mockRejectedValue(new Error("Invalid image"));
    await expect(writeClipboardImage("data:image/png;base64,bad")).rejects.toThrow("Invalid image");
  });
});
