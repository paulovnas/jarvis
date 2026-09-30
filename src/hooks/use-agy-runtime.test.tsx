import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useAgyRuntime } from "./use-agy-runtime";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

it("shares metadata, discovers imported activations, and avoids duplicate discovery while saving", async () => {
  const first = { installed: true, authenticated: false, version: "2", error: null, models: [], preferences: { enabled: false, disabledModels: [] } };
  vi.mocked(invoke).mockResolvedValue(first);
  const dormant = renderHook(() => useAgyRuntime(false));
  expect(invoke).not.toHaveBeenCalled();
  const selectors = renderHook(() => [useAgyRuntime(), useAgyRuntime()]);
  await waitFor(() => expect(selectors.result.current[0].data).toEqual(first));
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_agy_runtime");
  expect(dormant.result.current.data).toEqual(first);
  const connected = { ...first, authenticated: true, email: "person@example.test", models: [{ id: "default", name: "Padrão do CLI", description: "", reasoningLevels: [], defaultReasoning: null }] };
  vi.mocked(invoke).mockResolvedValue(connected);
  await act(async () => { await selectors.result.current[0].refresh(); });
  expect(invoke).toHaveBeenLastCalledWith("refresh_agy_runtime");
  expect(selectors.result.current[1].data).toEqual(connected);
  expect(dormant.result.current.data).toEqual(connected);
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "system:changed")?.[1];
  expect(changed).toBeDefined();
  const preferences = { enabled: false, disabledModels: ["default"] };
  act(() => changed?.({ event: "system:changed", id: 1, payload: { preferences: { agy: preferences } } }));
  expect(dormant.result.current.data?.preferences).toEqual(preferences);
  expect(selectors.result.current[1].data?.preferences).toEqual(preferences);
  act(() => changed?.({ event: "system:changed", id: 2, payload: null }));
  expect(dormant.result.current.data?.preferences).toEqual(preferences);
  const enabled = { enabled: true, showUsage: true, disabledModels: [] };
  vi.mocked(invoke).mockResolvedValue(first);
  await act(async () => { await selectors.result.current[0].refresh(); });
  expect(dormant.result.current.data?.models).toEqual([]);
  vi.mocked(invoke).mockResolvedValue({ ...connected, preferences: enabled });
  await act(async () => { changed?.({ event: "system:changed", id: 3, payload: { preferences: { agy: enabled } } }); });
  expect(invoke).toHaveBeenLastCalledWith("refresh_agy_runtime");
  expect(dormant.result.current.data).toEqual({ ...connected, preferences: enabled });
  act(() => changed?.({ event: "system:changed", id: 4, payload: { preferences: { agy: preferences } } }));
  const discoveryCount = vi.mocked(invoke).mock.calls.filter(([command]) => command === "refresh_agy_runtime").length;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === "save_agy_provider_preferences") {
      changed?.({ event: "system:changed", id: 5, payload: { preferences: { agy: enabled } } });
      return enabled;
    }
    return { ...connected, preferences: enabled };
  });
  await act(async () => { await selectors.result.current[0].savePreferences(enabled); });
  expect(invoke).toHaveBeenCalledWith("save_agy_provider_preferences", { preferences: enabled });
  expect(invoke).toHaveBeenLastCalledWith("refresh_agy_runtime");
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "refresh_agy_runtime")).toHaveLength(discoveryCount + 1);
  expect(dormant.result.current.data).toEqual({ ...connected, preferences: enabled });
});
