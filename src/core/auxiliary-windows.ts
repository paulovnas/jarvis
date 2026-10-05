import { invoke } from "@tauri-apps/api/core";

export type AuxiliaryWindowKind = "settings" | "about";

export function applicationSurface(label: string): "main" | "companion" | AuxiliaryWindowKind {
  return label === "companion" || label === "settings" || label === "about" ? label : "main";
}

/** Browser previews retain dialogs; native entry points use a single dedicated window. */
export async function openAuxiliaryWindow(kind: AuxiliaryWindowKind): Promise<boolean> {
  if (!("__TAURI_INTERNALS__" in window)) return false;
  await invoke("open_auxiliary_window", { kind });
  return true;
}

export const PROVIDER_SETTINGS_CHANGED = "settings:providers-changed";
