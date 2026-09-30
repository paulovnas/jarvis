import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { getAppShutdownStatus, installAppUpdate, type UpdateProgress } from "./app-update";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class { onmessage: ((message: UpdateProgress) => void) | undefined; },
}));

beforeEach(() => { vi.mocked(invoke).mockReset(); });

it("reads the current shutdown activity on each request instead of reusing a previous snapshot", async () => {
  vi.mocked(invoke)
    .mockResolvedValueOnce({ activeChats: 0, activeProcesses: 1, restartableProcesses: 1 })
    .mockResolvedValueOnce({ activeChats: 1, activeProcesses: 0, restartableProcesses: 0 });
  expect(await getAppShutdownStatus()).toEqual({ activeChats: 0, activeProcesses: 1, restartableProcesses: 1 });
  expect(await getAppShutdownStatus()).toEqual({ activeChats: 1, activeProcesses: 0, restartableProcesses: 0 });
  expect(invoke).toHaveBeenCalledTimes(2);
  expect(invoke).toHaveBeenLastCalledWith("get_app_shutdown_status");
});

it.each([
  { activeChats: 0, activeProcesses: -1, restartableProcesses: 0 },
  { activeChats: 0, activeProcesses: 0 },
  { activeChats: false, activeProcesses: 0, restartableProcesses: 0 },
])("rejects invalid activity status before authorizing installation: %j", async status => {
  vi.mocked(invoke).mockResolvedValue(status);
  await expect(getAppShutdownStatus()).rejects.toThrow();
});

it("authorizes stopping processes only when the installer caller explicitly confirms it", async () => {
  vi.mocked(invoke).mockResolvedValue(undefined);
  const progress = vi.fn();
  await installAppUpdate(progress);
  expect(invoke).toHaveBeenCalledWith("install_app_update", { onProgress: expect.objectContaining({ onmessage: progress }) });
  await installAppUpdate(progress, true);
  expect(invoke).toHaveBeenLastCalledWith("install_app_update", { onProgress: expect.objectContaining({ onmessage: progress }), stopProcesses: true });
});
