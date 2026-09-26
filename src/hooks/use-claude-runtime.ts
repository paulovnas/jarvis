import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { claudeProviderPreferencesSchema, claudeRuntimeSchema, type ClaudeProviderPreferences, type ClaudeRuntime } from "@/core/executors";
import { libraryError } from "@/core/library";

type Snapshot = { data: ClaudeRuntime | null; loading: boolean; saving: boolean; error: string | null };
let snapshot: Snapshot = { data: null, loading: false, saving: false, error: null };
let flight: Promise<void> | null = null;
let events: Promise<UnlistenFn> | null = null;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  events ??= listen<{ preferences?: { claude?: unknown } }>("system:changed", event => {
    const preferences = claudeProviderPreferencesSchema.safeParse(event.payload?.preferences?.claude);
    if (preferences.success && snapshot.data) update({ ...snapshot, data: { ...snapshot.data, preferences: preferences.data } });
  }).catch(() => () => {});
  return () => {
    listeners.delete(listener);
    if (!listeners.size) { void events?.then(stop => stop()); events = null; }
  };
};
const read = () => snapshot;
function update(next: Snapshot) { snapshot = next; listeners.forEach(listener => listener()); }
function refresh(force = false): Promise<void> {
  if (flight) return flight;
  update({ ...snapshot, loading: true, error: null });
  flight = invoke(force ? "refresh_claude_runtime" : "get_claude_runtime")
    .then(value => update({ ...snapshot, data: claudeRuntimeSchema.parse(value), loading: false, error: null }))
    .catch(cause => update({ ...snapshot, loading: false, error: libraryError(cause, "Não foi possível consultar o Claude Code.") }))
    .finally(() => { flight = null; });
  return flight;
}

async function savePreferences(preferences: ClaudeProviderPreferences): Promise<void> {
  if (snapshot.saving) return;
  update({ ...snapshot, saving: true });
  try {
    if (flight) await flight;
    const saved = claudeProviderPreferencesSchema.parse(await invoke("save_claude_provider_preferences", { preferences }));
    if (snapshot.data) update({ ...snapshot, data: { ...snapshot.data, preferences: saved } });
  } finally { update({ ...snapshot, saving: false }); }
}

export function useClaudeRuntime(enabled = true) {
  const state = useSyncExternalStore(subscribe, read, read);
  useEffect(() => { if (enabled && !snapshot.data && !snapshot.error) void refresh(); }, [enabled]);
  return { ...state, refresh: () => refresh(true), savePreferences };
}
