import { invoke } from "@tauri-apps/api/core";
import { afterEach, expect, it, vi } from "vitest";
import { applicationSurface, openAuxiliaryWindow } from "./auxiliary-windows";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });

it("keeps browser previews usable without requesting native windows", async () => {
  expect(await openAuxiliaryWindow("settings")).toBe(false);
  expect(invoke).not.toHaveBeenCalled();
});

it.each(["settings", "about"] as const)("opens the dedicated %s surface through native IPC", async kind => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  expect(await openAuxiliaryWindow(kind)).toBe(true);
  expect(invoke).toHaveBeenCalledWith("open_auxiliary_window", { kind });
  expect(applicationSurface(kind)).toBe(kind);
});

it("preserves the companion and main entry points", () => {
  expect(applicationSurface("companion")).toBe("companion");
  expect(applicationSurface("main")).toBe("main");
});
