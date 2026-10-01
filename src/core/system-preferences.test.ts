import { expect, it } from "vitest";
import { systemSnapshotSchema } from "./system-preferences";

it("reads older settings while dropping retired CLI preferences and preserving supported features", () => {
  const claude = { enabled: true, showUsage: true, disabledModels: ["hidden"] };
  const snapshot = systemSnapshotSchema.parse({
    preferences: { preventSleep: "active", notifications: true, companionEnabled: true, askUserTimeoutSeconds: 90, claude, agy: { enabled: true, showUsage: true, disabledModels: [] } },
    sleepInhibited: false, sleepError: null, notificationError: null,
  });
  expect(snapshot.preferences).not.toHaveProperty("agy");
  expect(snapshot.preferences.claude).toEqual(claude);
  expect(snapshot.preferences.notifications).toBe(true);
  expect(snapshot.preferences.companionEnabled).toBe(true);
  expect(snapshot.preferences.askUserTimeoutSeconds).toBe(90);
});
