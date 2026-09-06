export type ProviderModel = {
  id: string;
  name: string;
  reasoningLevels: string[];
  defaultReasoningLevel: string | null;
  contextWindow?: number | null;
};

export type ProviderAccount = {
  alias: string;
  providerKind: string;
  enabled: boolean;
  showUsage?: boolean;
  showThirdPartyUsage?: boolean;
  createdAt: number;
  email: string | null;
  accountType: "personal" | "enterprise" | "unknown";
  models: ProviderModel[];
  modelsAvailable: boolean;
};

function isProviderModel(value: unknown): value is ProviderModel {
  if (typeof value !== "object" || value === null) return false;
  const model = value as Partial<ProviderModel>;
  return (
    typeof model.id === "string" &&
    typeof model.name === "string" &&
    (model.contextWindow == null || (Number.isSafeInteger(model.contextWindow) && model.contextWindow > 0)) &&
    Array.isArray(model.reasoningLevels) &&
    model.reasoningLevels.every(
      (level: unknown) => typeof level === "string" && /^[a-z0-9_-]{1,32}$/.test(level),
    ) &&
    (model.defaultReasoningLevel === null ||
      (typeof model.defaultReasoningLevel === "string" &&
        model.reasoningLevels.includes(model.defaultReasoningLevel)))
  );
}

function isProviderAccount(value: unknown): value is ProviderAccount {
  if (typeof value !== "object" || value === null) return false;
  const account = value as Partial<ProviderAccount>;
  return (
    typeof account.alias === "string" &&
    typeof account.providerKind === "string" &&
    typeof account.enabled === "boolean" &&
    (account.showUsage === undefined || typeof account.showUsage === "boolean") &&
    (account.showThirdPartyUsage === undefined || typeof account.showThirdPartyUsage === "boolean") &&
    typeof account.createdAt === "number" &&
    (typeof account.email === "string" || account.email === null) &&
    (account.accountType === "personal" ||
      account.accountType === "enterprise" ||
      account.accountType === "unknown") &&
    Array.isArray(account.models) &&
    account.models.every(isProviderModel) &&
    typeof account.modelsAvailable === "boolean"
  );
}

export function accountList(value: unknown): ProviderAccount[] {
  return Array.isArray(value) ? value.filter(isProviderAccount) : [];
}
