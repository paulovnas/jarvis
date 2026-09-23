import { customConfigSchema, type CustomConfig } from "./custom-provider";

export type ProviderModel = {
  id: string;
  name: string;
  reasoningLevels: string[];
  defaultReasoningLevel: string | null;
  contextWindow?: number | null;
};

export type ProviderUsageAlert = {
  window: "five_hour" | "weekly";
  remainingPercent: number;
};

export type ProviderAccount = {
  alias: string;
  providerKind: string;
  enabled: boolean;
  showUsage?: boolean;
  showThirdPartyUsage?: boolean;
  usageAlert?: ProviderUsageAlert | null;
  createdAt: number;
  email: string | null;
  accountType: "personal" | "enterprise" | "unknown";
  models: ProviderModel[];
  modelsAvailable: boolean;
  disabledModels?: string[];
  custom?: CustomConfig;
};

function isUsageAlert(value: unknown): value is ProviderUsageAlert {
  if (typeof value !== "object" || value === null) return false;
  const alert = value as Partial<ProviderUsageAlert>;
  return (alert.window === "five_hour" || alert.window === "weekly")
    && typeof alert.remainingPercent === "number"
    && Number.isInteger(alert.remainingPercent)
    && alert.remainingPercent >= 1
    && alert.remainingPercent <= 100;
}

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
    (account.custom === undefined || customConfigSchema.safeParse(account.custom).success) &&
    typeof account.enabled === "boolean" &&
    (account.showUsage === undefined || typeof account.showUsage === "boolean") &&
    (account.showThirdPartyUsage === undefined || typeof account.showThirdPartyUsage === "boolean") &&
    (account.usageAlert == null || isUsageAlert(account.usageAlert)) &&
    typeof account.createdAt === "number" &&
    (typeof account.email === "string" || account.email === null) &&
    (account.accountType === "personal" ||
      account.accountType === "enterprise" ||
      account.accountType === "unknown") &&
    Array.isArray(account.models) &&
    account.models.every(isProviderModel) &&
    (account.disabledModels === undefined || (Array.isArray(account.disabledModels) && account.disabledModels.every((id: unknown) => typeof id === "string" && id.length > 0))) &&
    typeof account.modelsAvailable === "boolean"
  );
}

export function accountList(value: unknown): ProviderAccount[] {
  return Array.isArray(value) ? value.filter(isProviderAccount) : [];
}

export function enabledModels(account: ProviderAccount): ProviderModel[] {
  const disabled = new Set(account.disabledModels ?? []);
  return account.models.filter(model => !disabled.has(model.id));
}

export type ModelCatalogRefresh = {
  accounts: ProviderAccount[];
  refreshed: string[];
  failed: string[];
};

export function mergeModelCatalogRefresh(current: ProviderAccount[], fetched: ProviderAccount[]): ModelCatalogRefresh {
  const byAlias = new Map(fetched.map(account => [account.alias, account]));
  const refreshed: string[] = [];
  const failed: string[] = [];
  const accounts = current.map(account => {
    if (!account.enabled || !["openai-codex", "antigravity"].includes(account.providerKind)) return account;
    const next = byAlias.get(account.alias);
    if (!next || next.providerKind !== account.providerKind || !next.modelsAvailable) {
      failed.push(account.alias);
      return account;
    }
    refreshed.push(account.alias);
    return { ...account, models: next.models, modelsAvailable: true, disabledModels: next.disabledModels ?? account.disabledModels ?? [] };
  });
  return { accounts, refreshed, failed };
}
