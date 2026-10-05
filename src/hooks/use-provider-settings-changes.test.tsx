import { act, renderHook, waitFor } from "@testing-library/react";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { afterEach, expect, it, vi } from "vitest";
import { readResource } from "@/core/resource-request";
import { PROVIDER_SETTINGS_CHANGED } from "@/core/auxiliary-windows";
import { useProviderSettingsChanges } from "./use-provider-settings-changes";

vi.mock("@/core/resource-request", () => ({ readResource: vi.fn() }));
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });

it("refreshes models from native state when settings change and ignores outdated responses", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  let changed: EventCallback<unknown> | undefined;
  const dispose = vi.fn();
  vi.mocked(listen).mockImplementation(async (_name, callback) => { changed = callback; return dispose; });
  let resolveOld!: (accounts: unknown) => void;
  vi.mocked(readResource).mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; })).mockResolvedValueOnce([]);
  const update = vi.fn();
  const { unmount } = renderHook(() => useProviderSettingsChanges(update));
  await waitFor(() => expect(listen).toHaveBeenCalledWith(PROVIDER_SETTINGS_CHANGED, expect.any(Function)));
  await act(async () => {
    changed?.({ event: PROVIDER_SETTINGS_CHANGED, id: 1, payload: null });
    changed?.({ event: PROVIDER_SETTINGS_CHANGED, id: 2, payload: null });
  });
  expect(readResource).toHaveBeenCalledWith("list_provider_accounts", { cached: true });
  expect(update).toHaveBeenCalledExactlyOnceWith([]);
  await act(async () => { resolveOld([{ alias: "obsolete-provider", enabled: true, models: [] }]); });
  expect(update).toHaveBeenCalledOnce();
  unmount();
  expect(dispose).toHaveBeenCalledOnce();
});

it("does not subscribe to desktop events in a browser preview", () => {
  renderHook(() => useProviderSettingsChanges(vi.fn()));
  expect(listen).not.toHaveBeenCalled();
});
