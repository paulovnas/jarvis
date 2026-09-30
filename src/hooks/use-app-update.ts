import { useCallback, useEffect, useRef, useState } from "react";
import { ResourceTimeoutError, watchResourceRecovery } from "@/core/resource-request";
import { APP_VERSION, checkAppUpdate, getAppShutdownStatus, installAppUpdate, nativeUpdaterAvailable, type AppShutdownStatus, type UpdateInfo, type UpdateProgress } from "@/core/app-update";

const CHECK_INTERVAL = 6 * 60 * 60_000;
const failureMessage = (cause: unknown) => cause instanceof ResourceTimeoutError ? cause.message : typeof cause === "string" ? cause : "Não foi possível verificar a atualização. Tente novamente.";

export function useAppUpdate() {
  const [info, setInfo] = useState<UpdateInfo>({ currentVersion: APP_VERSION, available: null, installable: false });
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [upToDate, setUpToDate] = useState(false);
  const [pendingShutdown, setPendingShutdown] = useState<AppShutdownStatus | null>(null);
  const operation = useRef(false);
  const relaunchPending = useRef(false);
  const checkedAt = useRef(0);
  const checkFailed = useRef(false);
  const mounted = useRef(true);

  const check = useCallback(async (manual = false) => {
    if (!nativeUpdaterAvailable() || operation.current || relaunchPending.current) return;
    operation.current = true; setChecking(true); setError(null); setUpToDate(false);
    try {
      const next = await checkAppUpdate();
      checkFailed.current = false;
      if (mounted.current) { setInfo(next); setProgress(null); setUpToDate(manual && next.available === null); }
    } catch (cause) { checkFailed.current = true; if (mounted.current) setError(failureMessage(cause)); }
    finally {
      checkedAt.current = Date.now(); operation.current = false;
      if (mounted.current) setChecking(false);
    }
  }, []);
  const runInstall = useCallback(async (stopProcesses = false) => {
    if (operation.current) return;
    operation.current = true; setBusy(true); setError(null);
    try {
      if (!stopProcesses) {
        const shutdown = await getAppShutdownStatus();
        if (!mounted.current) return;
        if (shutdown.activeChats > 0) throw "Aguarde as execuções dos chats terminarem antes de atualizar.";
        if (shutdown.activeProcesses > 0) { setPendingShutdown(shutdown); return; }
      }
      setPendingShutdown(null);
      const report = (next: UpdateProgress) => {
        if (next.stage === "restarting") relaunchPending.current = true;
        if (mounted.current) setProgress(next);
      };
      if (stopProcesses) await installAppUpdate(report, true);
      else await installAppUpdate(report);
    }
    catch (cause) { if (mounted.current) setError(failureMessage(cause)); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  }, []);
  const install = useCallback(() => runInstall(), [runInstall]);
  const confirmInstall = useCallback(async () => { if (pendingShutdown) await runInstall(true); }, [pendingShutdown, runInstall]);
  const cancelInstall = useCallback(() => { if (!operation.current) setPendingShutdown(null); }, []);
  useEffect(() => {
    mounted.current = true;
    if (!nativeUpdaterAvailable()) return () => { mounted.current = false; };
    const timer = window.setTimeout(() => void check(), 2000);
    const interval = window.setInterval(() => void check(), CHECK_INTERVAL);
    const focus = () => { if (Date.now() - checkedAt.current >= CHECK_INTERVAL) void check(); };
    window.addEventListener("focus", focus);
    const stopRecovery = watchResourceRecovery(() => { if (checkFailed.current) void check(); });
    return () => { mounted.current = false; clearTimeout(timer); clearInterval(interval); stopRecovery(); window.removeEventListener("focus", focus); };
  }, [check]);
  return { info, checking, busy, progress, error, upToDate, pendingShutdown, check, install, confirmInstall, cancelInstall };
}
