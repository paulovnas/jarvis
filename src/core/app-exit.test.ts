import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { cancelAppExit, confirmAppExit, getPendingAppExit } from "./app-exit";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

it("distinguishes no pending close request from its current validated activity", async () => {
  const activity = { activeChats: 1, activeProcesses: 2, restartableProcesses: 1 };
  vi.mocked(invoke).mockResolvedValueOnce(null).mockResolvedValueOnce(activity);
  expect(await getPendingAppExit()).toBeNull();
  expect(await getPendingAppExit()).toEqual(activity);
  expect(invoke).toHaveBeenCalledTimes(2);
  expect(invoke).toHaveBeenLastCalledWith("get_pending_app_exit");
});

it("does not treat an invalid pending activity response as an idle runtime", async () => {
  vi.mocked(invoke).mockResolvedValue({ activeChats: 0, activeProcesses: -1 });
  await expect(getPendingAppExit()).rejects.toThrow();
});

it("preserves native failures when confirming or cancelling a close request", async () => {
  vi.mocked(invoke).mockRejectedValueOnce("Não foi possível salvar.").mockRejectedValueOnce("Não foi possível cancelar.");
  await expect(confirmAppExit()).rejects.toBe("Não foi possível salvar.");
  await expect(cancelAppExit()).rejects.toBe("Não foi possível cancelar.");
  expect(invoke).toHaveBeenNthCalledWith(1, "confirm_app_exit");
  expect(invoke).toHaveBeenNthCalledWith(2, "cancel_app_exit");
});
