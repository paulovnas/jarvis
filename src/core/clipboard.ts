import { readText as readNativeClipboardText, writeText as writeNativeClipboardText, writeImage as writeNativeClipboardImage } from "@tauri-apps/plugin-clipboard-manager";
import { Image as NativeImage } from "@tauri-apps/api/image";

export function nativeClipboardAvailable() {
  return "__TAURI_INTERNALS__" in window;
}

export async function writeClipboardText(text: string) {
  if (nativeClipboardAvailable()) {
    await writeNativeClipboardText(text);
    return;
  }
  if (!navigator.clipboard?.writeText) throw new Error("Clipboard API unavailable");
  await navigator.clipboard.writeText(text);
}

export async function readClipboardText() {
  if (nativeClipboardAvailable()) return readNativeClipboardText();
  if (!navigator.clipboard?.readText) throw new Error("Clipboard API unavailable");
  return navigator.clipboard.readText();
}

async function imageCanvas(source: string) {
  const image = new window.Image();
  image.src = source;
  await image.decode();
  if (!image.naturalWidth || !image.naturalHeight) throw new Error("Image unavailable");
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Image conversion unavailable");
  context.drawImage(image, 0, 0);
  return { canvas, context };
}

export async function writeClipboardImage(source: string) {
  if (nativeClipboardAvailable()) {
    const { canvas, context } = await imageCanvas(source);
    const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
    const image = await NativeImage.new(new Uint8Array(data), canvas.width, canvas.height);
    try { await writeNativeClipboardImage(image); }
    finally {
      // Cleanup must not report a failed copy after the clipboard was already written.
      await image.close().catch(() => undefined);
    }
    return;
  }
  if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined") throw new Error("Clipboard API unavailable");
  const png = imageCanvas(source).then(({ canvas }) => new Promise<Blob>((resolve, reject) => {
    canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error("Image conversion unavailable")), "image/png");
  }));
  // Supply a promised PNG immediately to preserve the browser's user activation.
  await Promise.all([navigator.clipboard.write([new ClipboardItem({ "image/png": png })]), png]);
}
