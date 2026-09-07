import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

export function useSettingsMenu(open: (value: boolean) => void) {
  useEffect(() => {
    let active = true;
    let cleanup: (() => void) | undefined;
    void listen("app:settings", () => { if (active) open(true); }).then(unlisten => {
      if (active) cleanup = unlisten; else unlisten();
    }).catch(() => { /* Browser preview has no native menu. */ });
    return () => { active = false; cleanup?.(); };
  }, [open]);
}
