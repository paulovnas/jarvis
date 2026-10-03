import { afterEach, expect, it, vi } from "vitest";
import { extensionApi, isFirefox, restrictStorageAccess } from "./platform";

afterEach(() => vi.unstubAllGlobals());

it("uses Firefox promise APIs and leaves its trusted storage unchanged", async () => {
  const firefox = { runtime: { getURL: () => "moz-extension://a/" }, storage: { local: {}, session: {} } };
  vi.stubGlobal("browser", firefox);
  vi.stubGlobal("chrome", { runtime: {} });
  expect(extensionApi()).toBe(firefox);
  expect(isFirefox()).toBe(true);
  await expect(restrictStorageAccess()).resolves.toBeUndefined();
});

it("retains the Chromium API and restricts both storage areas", async () => {
  const local = vi.fn();
  const session = vi.fn();
  const chromium = { runtime: { getURL: () => "chrome-extension://a/" }, storage: { local: { setAccessLevel: local }, session: { setAccessLevel: session } } };
  vi.stubGlobal("browser", undefined);
  vi.stubGlobal("chrome", chromium);
  expect(extensionApi()).toBe(chromium);
  expect(isFirefox()).toBe(false);
  await restrictStorageAccess();
  expect(local).toHaveBeenCalledWith({ accessLevel: "TRUSTED_CONTEXTS" });
  expect(session).toHaveBeenCalledWith({ accessLevel: "TRUSTED_CONTEXTS" });
});
