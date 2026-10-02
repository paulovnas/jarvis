import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { remoteControlError, remoteStatusSchema, type RemoteStatus } from "@/core/remote-control";

async function boundedInvoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      invoke(command, args),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("O serviço não respondeu. Atualize o status antes de tentar novamente.")), 10_000); }),
    ]);
  } finally { clearTimeout(timer); }
}

export function useRemoteControl(open: boolean) {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const revision = useRef(0);
  const mounted = useRef(false);
  const mutation = useRef(false);

  const refresh = useCallback(async () => {
    const version = ++revision.current;
    try {
      const next = remoteStatusSchema.parse(await boundedInvoke("get_remote_status"));
      if (mounted.current && version === revision.current) { setStatus(next); setError(null); }
    } catch (cause) {
      if (mounted.current && version === revision.current) setError(remoteControlError(cause));
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    const subscription = listen("remote:changed", event => {
      const next = remoteStatusSchema.safeParse(event.payload);
      if (mounted.current && next.success) { ++revision.current; setStatus(next.data); setError(null); }
    });
    void subscription.catch(cause => { if (mounted.current) setError(remoteControlError(cause)); });
    queueMicrotask(() => { if (mounted.current) void refresh(); });
    return () => { mounted.current = false; void subscription.then(stop => stop()).catch(() => {}); };
  }, [refresh]);

  useEffect(() => {
    if (!open) return;
    queueMicrotask(() => { if (mounted.current) void refresh(); });
    const poll = window.setInterval(() => { if (!mutation.current && document.visibilityState !== "hidden") void refresh(); }, 5_000);
    window.addEventListener("focus", refresh);
    return () => { clearInterval(poll); window.removeEventListener("focus", refresh); };
  }, [open, refresh]);

  const run = async (command: string, args?: Record<string, unknown>) => {
    if (mutation.current) return false;
    mutation.current = true;
    setBusy(true); setError(null); ++revision.current;
    try {
      const next = remoteStatusSchema.parse(await boundedInvoke(command, args));
      if (mounted.current) { ++revision.current; setStatus(next); }
      return true;
    } catch (cause) {
      if (mounted.current) setError(remoteControlError(cause));
      return false;
    } finally {
      mutation.current = false;
      if (mounted.current) setBusy(false);
    }
  };

  return { status, error, busy, refresh, run };
}
