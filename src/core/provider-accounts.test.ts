import { describe, expect, it } from "vitest";
import { accountList, type ProviderAccount } from "./provider-accounts";

const account: ProviderAccount = {
  alias: "openai-codex-pessoal", providerKind: "openai-codex", enabled: true, createdAt: 1,
  email: null, accountType: "personal", modelsAvailable: true,
  models: [{ id: "compact", name: "Compact", reasoningLevels: ["medium", "xhigh"], defaultReasoningLevel: "medium" }],
};

describe("provider account IPC validation", () => {
  it("retains the model's reported levels and default", () => {
    expect(accountList([account])).toEqual([account]);
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
});
