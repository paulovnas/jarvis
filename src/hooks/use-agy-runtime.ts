import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { agyProviderPreferencesSchema, agyRuntimeSchema, type AgyProviderPreferences, type AgyRuntime } from "@/core/agy";
import { libraryError } from "@/core/library";

type Snapshot = { data: AgyRuntime | null; loading: boolean; saving: boolean; error: string | null };
let snapshot: Snapshot = { data: null, loading: false, saving: false, error: null };
let flight: Promise<void> | null = null;
let events: Promise<UnlistenFn> | null = null;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  events ??= listen<{ preferences?: { agy?: unknown } }>("system:changed", event => {
    const preferences = agyProviderPreferencesSchema.safeParse(event.payload?.preferences?.agy);
    if (preferences.success && snapshot.data) {
      const discover = preferences.data.enabled && snapshot.data.preferences?.enabled !== true && !snapshot.saving;
      update({ ...snapshot, data: { ...snapshot.data, preferences: preferences.data } });
      if (discover) void refresh(true);
    }
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
  flight = invoke(force ? "refresh_agy_runtime" : "get_agy_runtime")
    .then(value => update({ ...snapshot, data: agyRuntimeSchema.parse(value), loading: false, error: null }))
    .catch(cause => update({ ...snapshot, loading: false, error: libraryError(cause, "Não foi possível consultar o Antigravity CLI.") }))
    .finally(() => { flight = null; });
  return flight;
}

async function savePreferences(preferences: AgyProviderPreferences): Promise<void> {
  if (snapshot.saving) return;
  update({ ...snapshot, saving: true });
  try {
    if (flight) await flight;
    const discover = preferences.enabled && snapshot.data?.preferences?.enabled !== true;
    const saved = agyProviderPreferencesSchema.parse(await invoke("save_agy_provider_preferences", { preferences }));
    if (snapshot.data) update({ ...snapshot, data: { ...snapshot.data, preferences: saved } });
    if (discover) await refresh(true);
  } finally { update({ ...snapshot, saving: false }); }
}

export function useAgyRuntime(enabled = true) {
  const state = useSyncExternalStore(subscribe, read, read);
  useEffect(() => { if (enabled && !snapshot.data && !snapshot.error) void refresh(); }, [enabled]);
  return { ...state, refresh: () => refresh(true), savePreferences };
}
