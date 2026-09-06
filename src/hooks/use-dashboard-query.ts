import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { z } from "zod";
import { dashboardError } from "@/core/dashboard";

export function useDashboardQuery<T>(command: string, projectId: string, schema: z.ZodType<T>, issueId?: string) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const generation = useRef(0);
  const flight = useRef(false);
  const refresh = useCallback(async () => {
    if (flight.current) return;
    flight.current = true;
    const version = generation.current;
    setLoading(true);
    try {
      const value = schema.parse(await invoke(command, { projectId, ...(issueId ? { issueId } : {}) }));
      if (generation.current === version) { setData(value); setError(null); }
    } catch (cause) { if (generation.current === version) setError(dashboardError(cause)); }
    finally { if (generation.current === version) { flight.current = false; setLoading(false); } }
  }, [command, projectId, schema, issueId]);
  useEffect(() => {
    const stops: (() => void)[] = [];
    let active = true;
    queueMicrotask(() => { if (active) void refresh(); });
    for (const event of ["library:changed", "beads:changed"]) {
      void listen(event, () => { if (active) void refresh(); }).then(stop => { if (active) stops.push(stop); else stop(); }).catch(() => {});
    }
    const interval = window.setInterval(() => { if (document.visibilityState === "visible") void refresh(); }, 20_000);
    const focus = () => { void refresh(); };
    window.addEventListener("focus", focus);
    return () => { active = false; generation.current += 1; flight.current = false; stops.forEach(stop => stop()); clearInterval(interval); window.removeEventListener("focus", focus); };
  }, [refresh]);
  return { data, error, loading, refresh };
}
