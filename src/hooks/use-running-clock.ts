import { useEffect, useState } from "react";

/** A one-second UI clock that exists only while an execution is active. */
export function useRunningClock(running: boolean) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!running) return;
    const tick = () => setNow(Date.now());
    tick();
    const timer = window.setInterval(tick, 1_000);
    return () => window.clearInterval(timer);
  }, [running]);

  return now;
}

export function executionDuration(startedAt: number, durationMs: number, running: boolean, now: number) {
  return running ? Math.max(durationMs, now - startedAt, 0) : Math.max(durationMs, 0);
}

export function formatExecutionDuration(durationMs: number) {
  const seconds = Math.max(0, Math.floor(durationMs / 1_000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${String(minutes % 60).padStart(2, "0")}m`;
}
