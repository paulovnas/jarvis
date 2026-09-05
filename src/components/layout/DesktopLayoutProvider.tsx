import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { HomeSkeleton } from "./LoadingSkeletons";
import { DEFAULT_DESKTOP_LAYOUT, DesktopLayoutContext, type DesktopLayout, type LayoutUpdate } from "@/core/desktop-layout";

export function DesktopLayoutProvider({ children }: { children: ReactNode }) {
  const [layout, setLayout] = useState(DEFAULT_DESKTOP_LAYOUT);
  const [ready, setReady] = useState(false);
  const current = useRef(layout);
  const writes = useRef(Promise.resolve());
  const writable = useRef(false);
  const errorShown = useRef(false);
  const reportError = useCallback(() => {
    if (!errorShown.current) {
      errorShown.current = true;
      toast.error("Não foi possível salvar o layout da janela");
    }
  }, []);

  useEffect(() => {
    let active = true;
    void invoke<DesktopLayout>("get_desktop_layout").then(saved => {
      if (!active) return;
      current.current = { ...DEFAULT_DESKTOP_LAYOUT, ...saved };
      setLayout(current.current);
      writable.current = true;
    }, () => {
      if (active) toast.error("Não foi possível restaurar o layout");
    }).finally(() => { if (active) setReady(true); });
    return () => { active = false; };
  }, []);

  const updateLayout = useCallback((update: LayoutUpdate) => {
    const next = { ...current.current, ...(typeof update === "function" ? update(current.current) : update) };
    current.current = next;
    setLayout(next);
    if (!writable.current) { reportError(); return; }
    // Ordered writes preserve quick changes to different controls; no unload debounce to lose.
    writes.current = writes.current.then(() => invoke<void>("save_desktop_layout", { layout: next })).then(() => { errorShown.current = false; }, reportError);
  }, [reportError]);

  return <DesktopLayoutContext value={{ layout, updateLayout }}>{ready ? children : <HomeSkeleton />}</DesktopLayoutContext>;
}
