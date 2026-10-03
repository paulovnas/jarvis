import { describe, expect, it } from "vitest";
import { accountList, enabledModels, mergeModelCatalogRefresh, type ProviderAccount } from "./provider-accounts";

const account: ProviderAccount = {
  alias: "openai-codex-pessoal", providerKind: "openai-codex", enabled: true, createdAt: 1,
  email: null, accountType: "personal", modelsAvailable: true,
  models: [{ id: "compact", name: "Compact", reasoningLevels: ["medium", "xhigh"], defaultReasoningLevel: "medium" }],
};

describe("provider account IPC validation", () => {
  it("preserves per-window visibility choices and reads legacy accounts", () => {
    expect(accountList([account])).toEqual([account]);
    const configured = { ...account, showFiveHourUsage: false, showWeeklyUsage: true };
    expect(accountList([configured])).toEqual([configured]);
    expect(accountList([{ ...account, showFiveHourUsage: true, showWeeklyUsage: false }])).toHaveLength(1);
  });

  it.each([
    { showFiveHourUsage: "false" },
    { showWeeklyUsage: 0 },
    { showFiveHourUsage: null },
    { showWeeklyUsage: null },
  ])("rejects invalid usage window preferences: %j", (visibility) => {
    expect(accountList([{ ...account, ...visibility }])).toEqual([]);
  });

  it("retains the model's reported levels and default", () => {
    const configured = { ...account, usageAlert: { window: "weekly" as const, remainingPercent: 20 } };
    expect(accountList([configured])).toEqual([configured]);
  });

  it.each([
    { reasoningLevels: true, defaultReasoningLevel: null },
    { reasoningLevels: ["medium", 2], defaultReasoningLevel: null },
    { reasoningLevels: ["medium"], defaultReasoningLevel: "high" },
    { reasoningLevels: ["medium", "<invalid>"], defaultReasoningLevel: null },
    { reasoningLevels: [], defaultReasoningLevel: "high" },
  ])("rejects invalid model capabilities at the IPC boundary: %j", (capabilities) => {
    expect(accountList([{ ...account, models: [{ ...account.models[0], ...capabilities }] }])).toEqual([]);
  });

  it("handles unavailable account payloads", () => {
    expect(accountList(undefined)).toEqual([]);
    expect(accountList({ accounts: [account] })).toEqual([]);
  });

  it("accepts only valid hidden-model IDs and excludes them from selection", () => {
    expect(accountList([{ ...account, disabledModels: ["compact"] }])).toHaveLength(1);
    expect(accountList([{ ...account, disabledModels: [2] }])).toEqual([]);
    expect(enabledModels({ ...account, disabledModels: ["compact"] })).toEqual([]);
  });

  it("validates optional vision capabilities without requiring them from older providers", () => {
    expect(accountList([account])).toHaveLength(1);
    expect(accountList([{ ...account, visionModels: ["compact"] }])).toHaveLength(1);
    expect(accountList([{ ...account, visionModels: [2] }])).toEqual([]);
  });

  it.each([
    { window: "daily", remainingPercent: 20 },
    { window: "weekly", remainingPercent: 0 },
    { window: "five_hour", remainingPercent: 101 },
    { window: "weekly", remainingPercent: 20.5 },
  ])("rejects invalid usage alert settings at the IPC boundary: %j", (usageAlert) => {
    expect(accountList([{ ...account, usageAlert }])).toEqual([]);
  });
});

describe("model catalog refresh", () => {
  it("preserves statusbar window choices across model refreshes", () => {
    const configured = { ...account, showFiveHourUsage: false, showWeeklyUsage: true };
    const refreshed = { ...account, showFiveHourUsage: true, showWeeklyUsage: false };
    expect(mergeModelCatalogRefresh([configured], [refreshed]).accounts[0]).toMatchObject({ showFiveHourUsage: false, showWeeklyUsage: true });
  });
  it("refreshes Go catalogs and retains their per-model visibility settings", () => {
    const go = { ...account, alias: "opencode-go-pessoal", providerKind: "opencode-go", disabledModels: ["compact"] };
    const model = { id: "glm", name: "GLM", reasoningLevels: [], defaultReasoningLevel: null };
    const result = mergeModelCatalogRefresh([go], [{ ...go, models: [model] }]);
    expect(result.refreshed).toEqual([go.alias]);
    expect(result.accounts[0]).toMatchObject({ models: [model], disabledModels: ["compact"] });
  });
  it("updates Codex and Antigravity independently while preserving failed and custom catalogs", () => {
    const antigravity = { ...account, alias: "antigravity-pessoal", providerKind: "antigravity" };
    const custom = { ...account, alias: "custom-local", providerKind: "custom" };
    const newCodexModel = { id: "gpt-6-sol", name: "GPT 6 Sol", reasoningLevels: ["high"], defaultReasoningLevel: "high" };
    const result = mergeModelCatalogRefresh(
      [account, antigravity, custom],
      [{ ...account, models: [newCodexModel] }, { ...antigravity, modelsAvailable: false, models: [] }, { ...custom, models: [newCodexModel] }],
    );
    expect(result.refreshed).toEqual([account.alias]);
    expect(result.failed).toEqual([antigravity.alias]);
    expect(result.accounts[0].models).toEqual([newCodexModel]);
    expect(result.accounts[1]).toEqual(antigravity);
    expect(result.accounts[2]).toEqual(custom);
  });

  it("retires models from an explicitly empty catalog and preserves a failed lookup", () => {
    const disabled = { ...account, alias: "openai-codex-off", enabled: false };
    const result = mergeModelCatalogRefresh([account, disabled], [{ ...account, models: [] }]);
    expect(result.accounts[0].models).toEqual([]);
    expect(result.accounts[1]).toEqual(disabled);
    expect(result.refreshed).toEqual([account.alias]);
    expect(result.failed).toEqual([]);
    const failed = mergeModelCatalogRefresh([account], [{ ...account, modelsAvailable: false, models: [] }]);
    expect(failed.accounts).toEqual([account]);
    expect(failed.failed).toEqual([account.alias]);
  });

  it("only reports a fresh catalog as refreshed and clears its offline marker", () => {
    const stale = { ...account, modelsStale: true };
    const failed = mergeModelCatalogRefresh([stale], [stale]);
    expect(failed.failed).toEqual([account.alias]);
    expect(failed.refreshed).toEqual([]);
    const fresh = mergeModelCatalogRefresh([stale], [account]);
    expect(fresh.refreshed).toEqual([account.alias]);
    expect(fresh.accounts[0].modelsStale).toBe(false);
  });
});
