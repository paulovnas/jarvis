import { writeText as writeNativeClipboardText } from "@tauri-apps/plugin-clipboard-manager";

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
