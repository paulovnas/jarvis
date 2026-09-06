import { useCallback, useEffect, useRef, useState } from "react";
import { APP_VERSION, checkAppUpdate, installAppUpdate, nativeUpdaterAvailable, type UpdateInfo, type UpdateProgress } from "@/core/app-update";

const CHECK_INTERVAL = 6 * 60 * 60_000;
const failureMessage = (cause: unknown) => typeof cause === "string" ? cause : "Não foi possível verificar a atualização. Tente novamente.";

export function useAppUpdate() {
  const [info, setInfo] = useState<UpdateInfo>({ currentVersion: APP_VERSION, available: null, installable: false });
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [upToDate, setUpToDate] = useState(false);
  const operation = useRef(false);
  const relaunchPending = useRef(false);
  const checkedAt = useRef(0);
  const mounted = useRef(true);

  const check = useCallback(async (manual = false) => {
    if (!nativeUpdaterAvailable() || operation.current || relaunchPending.current) return;
    operation.current = true; setChecking(true); setError(null); setUpToDate(false);
    try {
      const next = await checkAppUpdate();
      if (mounted.current) { setInfo(next); setProgress(null); setUpToDate(manual && next.available === null); }
    } catch (cause) { if (mounted.current) setError(failureMessage(cause)); }
    finally {
      checkedAt.current = Date.now(); operation.current = false;
      if (mounted.current) setChecking(false);
    }
  }, []);
  const install = useCallback(async () => {
    if (operation.current) return;
    operation.current = true; setBusy(true); setError(null);
    try { await installAppUpdate(next => {
      if (next.stage === "restarting") relaunchPending.current = true;
      if (mounted.current) setProgress(next);
    }); }
    catch (cause) { if (mounted.current) setError(failureMessage(cause)); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  }, []);
  useEffect(() => {
    mounted.current = true;
    if (!nativeUpdaterAvailable()) return () => { mounted.current = false; };
    const timer = window.setTimeout(() => void check(), 2000);
    const interval = window.setInterval(() => void check(), CHECK_INTERVAL);
    const focus = () => { if (Date.now() - checkedAt.current >= CHECK_INTERVAL) void check(); };
    window.addEventListener("focus", focus);
    return () => { mounted.current = false; clearTimeout(timer); clearInterval(interval); window.removeEventListener("focus", focus); };
  }, [check]);
  return { info, checking, busy, progress, error, upToDate, check, install };
}
