import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { getAppShutdownStatus, installAppUpdate, type AppShutdownStatus } from "@/core/app-update";
import { useAppUpdate } from "./use-app-update";

vi.mock("@/core/app-update", async original => ({ ...await original<typeof import("@/core/app-update")>(), nativeUpdaterAvailable: () => false, getAppShutdownStatus: vi.fn(), installAppUpdate: vi.fn() }));

beforeEach(() => {
  vi.mocked(getAppShutdownStatus).mockReset();
  vi.mocked(installAppUpdate).mockReset().mockResolvedValue(undefined);
});

it("admits only one installation while activity is being checked", async () => {
  let finish!: (status: AppShutdownStatus) => void;
  vi.mocked(getAppShutdownStatus).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const { result } = renderHook(() => useAppUpdate());
  let installing!: Promise<void>;
  act(() => { installing = result.current.install(); });
  await act(async () => { await result.current.install(); });
  expect(result.current.busy).toBe(true);
  expect(getAppShutdownStatus).toHaveBeenCalledOnce();
  await act(async () => { finish({ activeChats: 0, activeProcesses: 0, restartableProcesses: 0 }); await installing; });
  expect(installAppUpdate).toHaveBeenCalledOnce();
  expect(result.current.busy).toBe(false);
});

it("does not start installation if its controls unmount during the activity check", async () => {
  let finish!: (status: AppShutdownStatus) => void;
  vi.mocked(getAppShutdownStatus).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const { result, unmount } = renderHook(() => useAppUpdate());
  let installing!: Promise<void>;
  act(() => { installing = result.current.install(); });
  unmount();
  await act(async () => { finish({ activeChats: 0, activeProcesses: 0, restartableProcesses: 0 }); await installing; });
  expect(installAppUpdate).not.toHaveBeenCalled();
});
