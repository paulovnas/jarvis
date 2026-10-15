import { expect, it } from "vitest";
import { browserPreferencesSchema, systemSnapshotSchema } from "./system-preferences";
import { DEFAULT_CLAUDE_PREFERENCES } from "./executors";

it("defaults close behavior to tray for old preferences and accepts only supported choices", () => {
  const input = { preferences: { preventSleep: "off", notifications: false, askUserTimeoutSeconds: 30 }, sleepInhibited: false, sleepError: null, notificationError: null };
  expect(systemSnapshotSchema.parse(input).preferences).toHaveProperty("closeBehavior", "tray");
  expect(systemSnapshotSchema.parse({ ...input, preferences: { ...input.preferences, closeBehavior: "exit" } }).preferences).toHaveProperty("closeBehavior", "exit");
  expect(systemSnapshotSchema.safeParse({ ...input, preferences: { ...input.preferences, closeBehavior: "invalid" } }).success).toBe(false);
});

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

it("preserves Firefox selection while old browser preferences keep their Chromium defaults", () => {
  expect(browserPreferencesSchema.parse({})).toEqual({ mode: "embedded", application: "chrome" });
  expect(browserPreferencesSchema.parse({ mode: "extension", application: "firefox" })).toEqual({ mode: "extension", application: "firefox" });
});

it("preserves independent Claude statusbar windows in system settings", () => {
  expect(DEFAULT_CLAUDE_PREFERENCES).toMatchObject({ showFiveHourUsage: true, showWeeklyUsage: true });
  const claude = { enabled: true, showUsage: true, showFiveHourUsage: false, showWeeklyUsage: true, disabledModels: [] };
  const snapshot = systemSnapshotSchema.parse({
    preferences: { preventSleep: "active", notifications: true, askUserTimeoutSeconds: 90, claude },
    sleepInhibited: false, sleepError: null, notificationError: null,
  });
  expect(snapshot.preferences.claude).toEqual(claude);
});
