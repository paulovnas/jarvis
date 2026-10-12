import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.resetModules();
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockClear();
});

it("shares one metadata probe across selectors and refreshes all consumers explicitly", async () => {
  const { useClaudeRuntime } = await import("./use-claude-runtime");
  const first = { installed: true, authenticated: false, version: "2", error: null, models: [{ id: "default", name: "Padrão do CLI", description: "", reasoningLevels: [], defaultReasoning: null }] };
  vi.mocked(invoke).mockResolvedValue(first);
  const dormant = renderHook(() => useClaudeRuntime(false));
  expect(invoke).not.toHaveBeenCalled();
  const selectors = renderHook(() => [useClaudeRuntime(), useClaudeRuntime()]);
  await waitFor(() => expect(selectors.result.current[0].data).toEqual(first));
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_runtime");
  expect(dormant.result.current.data).toEqual(first);
  const connected = { ...first, authenticated: true, email: "person@example.test" };
  vi.mocked(invoke).mockResolvedValue(connected);
  await act(async () => { await selectors.result.current[0].refresh(); });
  expect(invoke).toHaveBeenLastCalledWith("refresh_claude_runtime");
  expect(selectors.result.current[1].data).toEqual(connected);
  expect(dormant.result.current.data).toEqual(connected);
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "system:changed")?.[1];
  expect(changed).toBeDefined();
  const preferences = { enabled: false, disabledModels: ["default"] };
  act(() => changed?.({ event: "system:changed", id: 1, payload: { preferences: { claude: preferences } } }));
  expect(dormant.result.current.data?.preferences).toEqual(preferences);
  expect(selectors.result.current[1].data?.preferences).toEqual(preferences);
  act(() => changed?.({ event: "system:changed", id: 2, payload: null }));
  expect(dormant.result.current.data?.preferences).toEqual(preferences);
});

it("updates every consumer from a refresh in another native window without another CLI probe", async () => {
  const { useClaudeRuntime } = await import("./use-claude-runtime");
  const stale = { installed: true, authenticated: false, version: "2", models: [], error: "Login necessário" };
  vi.mocked(invoke).mockResolvedValue(stale);
  const settings = renderHook(() => useClaudeRuntime());
  const composer = renderHook(() => useClaudeRuntime(false));
  await waitFor(() => expect(composer.result.current.data).toEqual(stale));
  const fresh = { ...stale, authenticated: true, error: null, email: "new@example.test", models: [{ id: "sonnet-new", name: "Sonnet novo", description: "", reasoningLevels: ["high"], defaultReasoning: "high" }] };
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "claude-runtime:changed")?.[1];
  expect(changed).toBeDefined();
  act(() => changed?.({ event: "claude-runtime:changed", id: 1, payload: fresh }));
  expect(composer.result.current.data).toEqual(fresh);
  expect(settings.result.current.data).toEqual(fresh);
  expect(composer.result.current.error).toBeNull();
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_runtime");
  act(() => changed?.({ event: "claude-runtime:changed", id: 2, payload: { models: "invalid" } }));
  expect(composer.result.current.data).toEqual(fresh);
});

it.each(["resolve", "reject"] as const)("keeps a fresh native event when an older metadata request later %ss", async outcome => {
  const { useClaudeRuntime } = await import("./use-claude-runtime");
  const stale = { installed: false, authenticated: false, version: null, models: [], error: "CLI não encontrado" };
  const fresh = { ...stale, installed: true, authenticated: true, version: "3", error: null };
  let resolve: (value: typeof stale) => void = () => {};
  let reject: (cause: Error) => void = () => {};
  const pending = new Promise<typeof stale>((complete, fail) => { resolve = complete; reject = fail; });
  vi.mocked(invoke).mockReturnValue(pending);
  const composer = renderHook(() => useClaudeRuntime());
  await waitFor(() => expect(composer.result.current.loading).toBe(true));
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "claude-runtime:changed")?.[1];
  act(() => changed?.({ event: "claude-runtime:changed", id: 1, payload: fresh }));
  expect(composer.result.current.data).toEqual(fresh);
  expect(composer.result.current.loading).toBe(false);
  await act(async () => { if (outcome === "resolve") resolve(stale); else reject(new Error("old probe failed")); });
  expect(composer.result.current.data).toEqual(fresh);
  expect(composer.result.current.error).toBeNull();
  expect(composer.result.current.loading).toBe(false);
});

it("does not lose an explicit refresh requested during cached startup discovery", async () => {
  const { useClaudeRuntime } = await import("./use-claude-runtime");
  const stale = { installed: false, authenticated: false, version: null, models: [], error: null };
  const fresh = { ...stale, installed: true, authenticated: true, version: "3" };
  let resolve: (value: typeof stale) => void = () => {};
  const pending = new Promise<typeof stale>(complete => { resolve = complete; });
  vi.mocked(invoke).mockImplementation(command => command === "get_claude_runtime" ? pending : Promise.resolve(fresh));
  const composer = renderHook(() => useClaudeRuntime());
  await waitFor(() => expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_runtime"));
  let refresh: Promise<void> | undefined;
  act(() => { refresh = composer.result.current.refresh(); });
  expect(invoke).not.toHaveBeenCalledWith("refresh_claude_runtime");
  await act(async () => { resolve(stale); await refresh; });
  expect(invoke).toHaveBeenLastCalledWith("refresh_claude_runtime");
  expect(vi.mocked(invoke).mock.calls).toHaveLength(2);
  expect(composer.result.current.data).toEqual(fresh);
});

it.each(["startup", "refresh", "event"] as const)("keeps newer settings-window preferences while accepting %s model discovery", async phase => {
  const { useClaudeRuntime } = await import("./use-claude-runtime");
  const oldPreferences = { enabled: true, disabledModels: [] as string[] };
  const stale = { installed: true, authenticated: false, version: "2", models: [], error: null, preferences: oldPreferences };
  const fresh = { ...stale, authenticated: true, version: "3", models: [{ id: "sonnet-new", name: "Sonnet novo", description: "", reasoningLevels: ["high"], defaultReasoning: "high" }] };
  let resolve: (value: typeof fresh) => void = () => {};
  const pending = new Promise<typeof fresh>(complete => { resolve = complete; });
  vi.mocked(invoke).mockImplementation(command => command === "get_claude_runtime" && phase !== "startup" ? Promise.resolve(stale) : pending);
  const composer = renderHook(() => useClaudeRuntime());
  let refresh: Promise<void> | undefined;
  if (phase !== "startup") {
    await waitFor(() => expect(composer.result.current.data).toEqual(stale));
    act(() => { refresh = composer.result.current.refresh(); });
  }
  await waitFor(() => expect(composer.result.current.loading).toBe(true));
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "system:changed")?.[1];
  expect(changed).toBeDefined();
  const preferences = { enabled: false, disabledModels: ["sonnet-new"] };
  act(() => changed?.({ event: "system:changed", id: 1, payload: { preferences: { claude: preferences } } }));
  if (phase === "event") {
    const discovered = vi.mocked(listen).mock.calls.find(([name]) => name === "claude-runtime:changed")?.[1];
    act(() => discovered?.({ event: "claude-runtime:changed", id: 2, payload: fresh }));
  }
  await act(async () => { resolve(fresh); await refresh; });
  await waitFor(() => expect(composer.result.current.loading).toBe(false));
  expect(composer.result.current.data).toEqual({ ...fresh, preferences });
  expect(composer.result.current.error).toBeNull();

  // A discovery started after the settings event may return current preferences.
  vi.mocked(invoke).mockResolvedValue(fresh);
  await act(async () => { await composer.result.current.refresh(); });
  expect(composer.result.current.data).toEqual(fresh);
});

it("does not overwrite a newer settings-window save with an older save response", async () => {
  const { useClaudeRuntime } = await import("./use-claude-runtime");
  const first = { installed: true, authenticated: true, version: "2", models: [], error: null, preferences: { enabled: true, disabledModels: [] as string[] } };
  const oldSave = { enabled: true, disabledModels: ["sonnet"] };
  let resolve: (value: typeof oldSave) => void = () => {};
  const pending = new Promise<typeof oldSave>(complete => { resolve = complete; });
  vi.mocked(invoke).mockImplementation(command => command === "save_claude_provider_preferences" ? pending : Promise.resolve(first));
  const composer = renderHook(() => useClaudeRuntime());
  await waitFor(() => expect(composer.result.current.data).toEqual(first));
  let save: Promise<void> | undefined;
  act(() => { save = composer.result.current.savePreferences(oldSave); });
  await waitFor(() => expect(composer.result.current.saving).toBe(true));
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "system:changed")?.[1];
  const preferences = { enabled: false, disabledModels: ["sonnet", "haiku"] };
  act(() => changed?.({ event: "system:changed", id: 1, payload: { preferences: { claude: preferences } } }));
  await act(async () => { resolve(oldSave); await save; });
  expect(composer.result.current.data?.preferences).toEqual(preferences);
  expect(composer.result.current.saving).toBe(false);
});
