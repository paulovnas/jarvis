import { describe, expect, it } from "vitest";
import { accountList, type ProviderAccount } from "./provider-accounts";

const account: ProviderAccount = {
  alias: "openai-codex-pessoal", providerKind: "openai-codex", enabled: true, createdAt: 1,
  email: null, accountType: "personal", modelsAvailable: true,
  models: [{ id: "compact", name: "Compact", reasoningLevels: ["medium", "xhigh"], defaultReasoningLevel: "medium" }],
};

describe("provider account IPC validation", () => {
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

  it.each([
    { window: "daily", remainingPercent: 20 },
    { window: "weekly", remainingPercent: 0 },
    { window: "five_hour", remainingPercent: 101 },
    { window: "weekly", remainingPercent: 20.5 },
  ])("rejects invalid usage alert settings at the IPC boundary: %j", (usageAlert) => {
    expect(accountList([{ ...account, usageAlert }])).toEqual([]);
  });
});
