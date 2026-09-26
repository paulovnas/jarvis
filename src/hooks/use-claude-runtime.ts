import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { claudeRuntimeSchema, type ClaudeRuntime } from "@/core/executors";
import { libraryError } from "@/core/library";

type Snapshot = { data: ClaudeRuntime | null; loading: boolean; error: string | null };
let snapshot: Snapshot = { data: null, loading: false, error: null };
let flight: Promise<void> | null = null;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const read = () => snapshot;
function update(next: Snapshot) { snapshot = next; listeners.forEach(listener => listener()); }
function refresh(force = false): Promise<void> {
  if (flight) return flight;
  update({ ...snapshot, loading: true, error: null });
  flight = invoke(force ? "refresh_claude_runtime" : "get_claude_runtime")
    .then(value => update({ data: claudeRuntimeSchema.parse(value), loading: false, error: null }))
    .catch(cause => update({ ...snapshot, loading: false, error: libraryError(cause, "Não foi possível consultar o Claude Code.") }))
    .finally(() => { flight = null; });
  return flight;
}

export function useClaudeRuntime(enabled = true) {
  const state = useSyncExternalStore(subscribe, read, read);
  useEffect(() => { if (enabled && !snapshot.data && !snapshot.error) void refresh(); }, [enabled]);
  return { ...state, refresh: () => refresh(true) };
}
