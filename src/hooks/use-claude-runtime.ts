import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { claudeProviderPreferencesSchema, claudeRuntimeSchema, type ClaudeProviderPreferences, type ClaudeRuntime } from "@/core/executors";
import { libraryError } from "@/core/library";

type Snapshot = { data: ClaudeRuntime | null; loading: boolean; saving: boolean; error: string | null };
let snapshot: Snapshot = { data: null, loading: false, saving: false, error: null };
let flight: Promise<void> | null = null;
let flightForced = false;
let revision = 0;
let preferencesRevision = 0;
let latestPreferences: ClaudeProviderPreferences | null = null;
let flightPreferencesRevision: number | null = null;
let events: Promise<UnlistenFn[]> | null = null;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  events ??= Promise.all([
    listen<{ preferences?: { claude?: unknown } }>("system:changed", event => {
      const preferences = claudeProviderPreferencesSchema.safeParse(event.payload?.preferences?.claude);
      if (preferences.success) updatePreferences(preferences.data);
    }).catch(() => () => {}),
    listen<unknown>("claude-runtime:changed", event => {
      const status = claudeRuntimeSchema.safeParse(event.payload);
      if (!status.success) return;
      revision++;
      update({ ...snapshot, data: discovered(status.data, flightPreferencesRevision ?? preferencesRevision), loading: false, error: null });
    }).catch(() => () => {}),
  ]);
  return () => {
    listeners.delete(listener);
    if (!listeners.size) { void events?.then(stops => stops.forEach(stop => stop())); events = null; }
  };
};
const read = () => snapshot;
function update(next: Snapshot) { snapshot = next; listeners.forEach(listener => listener()); }
function updatePreferences(preferences: ClaudeProviderPreferences) {
  latestPreferences = preferences;
  preferencesRevision++;
  if (snapshot.data) update({ ...snapshot, data: { ...snapshot.data, preferences } });
}
function discovered(data: ClaudeRuntime, requestPreferencesRevision: number): ClaudeRuntime {
  return latestPreferences && preferencesRevision > requestPreferencesRevision ? { ...data, preferences: latestPreferences } : data;
}
function refresh(force = false): Promise<void> {
  if (flight) return force && !flightForced ? flight.then(() => refresh(true)) : flight;
  const request = ++revision;
  const requestPreferencesRevision = preferencesRevision;
  flightPreferencesRevision = requestPreferencesRevision;
  flightForced = force;
  update({ ...snapshot, loading: true, error: null });
  flight = invoke(force ? "refresh_claude_runtime" : "get_claude_runtime")
    .then(value => { if (request === revision) update({ ...snapshot, data: discovered(claudeRuntimeSchema.parse(value), requestPreferencesRevision), loading: false, error: null }); })
    .catch(cause => { if (request === revision) update({ ...snapshot, loading: false, error: libraryError(cause, "Não foi possível consultar o Claude Code.") }); })
    .finally(() => { flight = null; flightForced = false; flightPreferencesRevision = null; });
  return flight;
}

async function savePreferences(preferences: ClaudeProviderPreferences): Promise<void> {
  if (snapshot.saving) return;
  update({ ...snapshot, saving: true });
  try {
    if (flight) await flight;
    const requestPreferencesRevision = preferencesRevision;
    const saved = claudeProviderPreferencesSchema.parse(await invoke("save_claude_provider_preferences", { preferences }));
    if (requestPreferencesRevision === preferencesRevision) updatePreferences(saved);
  } finally { update({ ...snapshot, saving: false }); }
}

export function useClaudeRuntime(enabled = true) {
  const state = useSyncExternalStore(subscribe, read, read);
  useEffect(() => { if (enabled && !snapshot.data && !snapshot.error) void refresh(); }, [enabled]);
  return { ...state, refresh: () => refresh(true), savePreferences };
}
