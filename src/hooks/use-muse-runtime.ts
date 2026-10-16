import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { museProviderPreferencesSchema, museRuntimeSchema, type MuseProviderPreferences, type MuseRuntime } from "@/core/executors";
import { libraryError } from "@/core/library";

type Snapshot = { data: MuseRuntime | null; loading: boolean; saving: boolean; error: string | null };
let snapshot: Snapshot = { data: null, loading: false, saving: false, error: null };
let flight: Promise<void> | null = null;
let flightForced = false;
let revision = 0;
let preferencesRevision = 0;
let latestPreferences: MuseProviderPreferences | null = null;
let flightPreferencesRevision: number | null = null;
let events: Promise<UnlistenFn[]> | null = null;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  events ??= Promise.all([
    listen<{ preferences?: { muse?: unknown } }>("system:changed", event => {
      const preferences = museProviderPreferencesSchema.safeParse(event.payload?.preferences?.muse);
      if (preferences.success) updatePreferences(preferences.data);
    }).catch(() => () => {}),
    listen<unknown>("muse-runtime:changed", event => {
      const status = museRuntimeSchema.safeParse(event.payload);
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
function updatePreferences(preferences: MuseProviderPreferences) {
  latestPreferences = preferences;
  preferencesRevision++;
  if (snapshot.data) update({ ...snapshot, data: { ...snapshot.data, preferences } });
}
function discovered(data: MuseRuntime, requestPreferencesRevision: number): MuseRuntime {
  return latestPreferences && preferencesRevision > requestPreferencesRevision ? { ...data, preferences: latestPreferences } : data;
}
function refresh(force = false): Promise<void> {
  if (flight) return force && !flightForced ? flight.then(() => refresh(true)) : flight;
  const request = ++revision;
  const requestPreferencesRevision = preferencesRevision;
  flightPreferencesRevision = requestPreferencesRevision;
  flightForced = force;
  update({ ...snapshot, loading: true, error: null });
  flight = Promise.resolve(invoke(force ? "refresh_muse_runtime" : "get_muse_runtime"))
    .then(value => { if (request === revision) update({ ...snapshot, data: discovered(museRuntimeSchema.parse(value), requestPreferencesRevision), loading: false, error: null }); })
    .catch(cause => { if (request === revision) update({ ...snapshot, loading: false, error: libraryError(cause, "Não foi possível consultar o Muse Code.") }); })
    .finally(() => { flight = null; flightForced = false; flightPreferencesRevision = null; });
  return flight;
}

async function savePreferences(preferences: MuseProviderPreferences): Promise<void> {
  if (snapshot.saving) return;
  update({ ...snapshot, saving: true });
  try {
    if (flight) await flight;
    const requestPreferencesRevision = preferencesRevision;
    const saved = museProviderPreferencesSchema.parse(await invoke("set_muse_preferences", { preferences }));
    if (requestPreferencesRevision === preferencesRevision) updatePreferences(saved);
  } finally { update({ ...snapshot, saving: false }); }
}

export function useMuseRuntime(enabled = true) {
  const state = useSyncExternalStore(subscribe, read, read);
  useEffect(() => { if (enabled && !snapshot.data && !snapshot.error) void refresh(); }, [enabled]);
  return { ...state, refresh: () => refresh(true), savePreferences };
}
