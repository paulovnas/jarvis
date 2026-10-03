import { act, renderHook } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useOpencodeFreeModels } from "./use-opencode-free-models";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const free = [{ id: "space-bunny-free", name: "Space Bunny Free" }];
beforeEach(() => { vi.useFakeTimers(); vi.mocked(invoke).mockReset(); });
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

it("confirms free models, clears their labels after a failed refresh, and recovers", async () => {
  vi.mocked(invoke).mockResolvedValueOnce(free).mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce([]);
  const { result, unmount } = renderHook(useOpencodeFreeModels);
  await act(async () => {});
  expect(result.current.models).toEqual(free);
  expect(invoke).toHaveBeenCalledWith("get_opencode_go_free_models");
  await act(() => vi.advanceTimersByTimeAsync(60_000));
  expect(result.current).toEqual({ models: null, error: true });
  await act(() => vi.advanceTimersByTimeAsync(5_001));
  await act(async () => { window.dispatchEvent(new Event("online")); });
  expect(result.current).toEqual({ models: [], error: false });
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});

it("ignores responses after leaving the popover and disposes recovery listeners", async () => {
  let finish!: (value: unknown) => void;
  vi.mocked(invoke).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  const { result, unmount } = renderHook(useOpencodeFreeModels);
  unmount();
  await act(async () => { finish(free); });
  expect(result.current.models).toBeNull();
  window.dispatchEvent(new Event("online"));
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(vi.getTimerCount()).toBe(0);
});

it("treats an invalid catalog as unknown rather than free or empty", async () => {
  vi.mocked(invoke).mockResolvedValue([{ id: "space-bunny-free", name: null }]);
  const { result } = renderHook(useOpencodeFreeModels);
  await act(async () => {});
  expect(result.current).toEqual({ models: null, error: true });
});

it("recovers a timed-out catalog without accepting the older result", async () => {
  let finish!: (value: unknown) => void;
  vi.mocked(invoke).mockReturnValueOnce(new Promise(resolve => { finish = resolve; })).mockResolvedValueOnce(free);
  const { result } = renderHook(useOpencodeFreeModels);
  await act(() => vi.advanceTimersByTimeAsync(45_001));
  expect(result.current.error).toBe(true);
  await act(async () => { window.dispatchEvent(new Event("online")); });
  expect(result.current.models).toEqual(free);
  await act(async () => { finish([]); });
  expect(result.current.models).toEqual(free);
});
