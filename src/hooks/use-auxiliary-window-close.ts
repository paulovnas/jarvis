import { useCallback, useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";

export function useAuxiliaryWindowClose() {
  const busy = useRef(false);
  const closeRequest = useRef<(() => void) | null>(null);
  const approved = useRef(false);
  const setBusy = useCallback((value: boolean) => { busy.current = value; }, []);
  const setCloseRequest = useCallback((handler: (() => void) | null) => { closeRequest.current = handler; }, []);
  const close = useCallback(() => {
    if (busy.current) return;
    approved.current = true;
    void getCurrentWindow().close().catch(() => { approved.current = false; toast.error("Não foi possível fechar a janela."); });
  }, []);
  useEffect(() => {
    let active = true;
    let dispose: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested(event => {
      if (approved.current) return;
      if (busy.current) event.preventDefault();
      else if (closeRequest.current) { event.preventDefault(); closeRequest.current(); }
    }).then(unlisten => { if (active) dispose = unlisten; else unlisten(); }).catch(() => {});
    return () => { active = false; dispose?.(); };
  }, []);
  return { close, setBusy, setCloseRequest };
}
