import { openUrl } from "@tauri-apps/plugin-opener";

const MAC_PLATFORM_PATTERN = /Mac|iPhone|iPad|iPod/i;

export function isTerminalLinkModifierPressed(event: Pick<MouseEvent, "ctrlKey" | "metaKey">, platform = navigator.platform) {
  return MAC_PLATFORM_PATTERN.test(platform) ? event.metaKey : event.ctrlKey;
}

export function resolveTerminalWebUrl(candidate: string) {
  try {
    const url = new URL(candidate);
    return url.protocol === "http:" || url.protocol === "https:" ? url.href : null;
  } catch {
    return null;
  }
}

export async function openTerminalLink(event: MouseEvent, candidate: string, platform = navigator.platform) {
  if (!isTerminalLinkModifierPressed(event, platform)) return false;
  const url = resolveTerminalWebUrl(candidate);
  if (!url) return false;
  event.preventDefault();
  await openUrl(url);
  return true;
}
