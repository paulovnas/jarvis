import { act, renderHook } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, expect, it, vi } from "vitest";
import { useProviderUsage } from "./use-provider-usage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

it("keeps usage visible during an outage and refreshes on reconnection after a stalled query", async () => {
  vi.useFakeTimers();
  const first = { alias: "account", fetchedAt: 1, email: null, plan: "plus", windows: [], error: null, resetCredits: null };
  vi.mocked(invoke).mockResolvedValueOnce(first).mockReturnValueOnce(new Promise(() => {})).mockResolvedValueOnce({ ...first, fetchedAt: 2 });
  const { result, unmount } = renderHook(() => useProviderUsage("account"));
  await act(async () => {});
  expect(result.current.data?.fetchedAt).toBe(1);
  await act(() => vi.advanceTimersByTimeAsync(61_000));
  await act(async () => { window.dispatchEvent(new Event("online")); });
  expect(invoke).toHaveBeenCalledTimes(2);
  await act(() => vi.advanceTimersByTimeAsync(45_001));
  expect(result.current.error).toBe(true);
  expect(result.current.data?.fetchedAt).toBe(1);
  await act(async () => { window.dispatchEvent(new Event("online")); });
  expect(result.current.error).toBe(false);
  expect(result.current.data?.fetchedAt).toBe(2);
  unmount();
});
