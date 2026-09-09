import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { ProviderAccount } from "@/core/provider-accounts";
import { ProviderUsageAlertSettings } from "./ProviderUsageAlertSettings";

const usage = vi.hoisted(() => vi.fn());
vi.mock("@/hooks/use-provider-usage", () => ({ useProviderUsage: usage }));

const account: ProviderAccount = {
  alias: "openai-codex-paulo", providerKind: "openai-codex", enabled: true, createdAt: 1,
  email: null, accountType: "personal", models: [], modelsAvailable: true,
};

beforeEach(() => usage.mockReturnValue({ data: { alias: account.alias, fetchedAt: 1, email: null, plan: null, error: null, resetCredits: null, windows: [
  { id: "short", group: "Codex", thirdParty: false, label: "5h", durationSeconds: 18_000, remainingPercent: 80, resetsAt: 2 },
  { id: "week", group: "Codex", thirdParty: false, label: "7d", durationSeconds: 604_800, remainingPercent: 80, resetsAt: 2 },
] }, error: false }));

it("enables an alert with a real available window and a clear remaining percentage", async () => {
  const user = userEvent.setup();
  const change = vi.fn();
  render(<ProviderUsageAlertSettings account={account} saving={false} onChange={change} />);
  expect(screen.getByText(/percentual restante/i)).toBeVisible();
  await user.click(screen.getByRole("switch", { name: "Alertar sobre limite" }));
  expect(change).toHaveBeenCalledWith(account.alias, { window: "weekly", remainingPercent: 20 });
});

it("changes only to returned windows and validates the threshold", async () => {
  const user = userEvent.setup();
  const change = vi.fn();
  const configured = { ...account, usageAlert: { window: "weekly" as const, remainingPercent: 20 } };
  render(<ProviderUsageAlertSettings account={configured} saving={false} onChange={change} />);
  await user.click(screen.getByRole("combobox", { name: "Janela" }));
  await user.click(await screen.findByRole("option", { name: "5 horas" }));
  expect(change).toHaveBeenCalledWith(account.alias, { window: "five_hour", remainingPercent: 20 });

  const threshold = screen.getByRole("spinbutton", { name: `Porcentagem restante para ${account.alias}` });
  await user.clear(threshold);
  await user.type(threshold, "0");
  await user.tab();
  expect(screen.getByRole("alert")).toHaveTextContent("entre 1 e 100");
  expect(threshold).toHaveValue(20);
});

it("does not offer a five-hour alert when the account did not return that window", () => {
  usage.mockReturnValue({ data: { alias: account.alias, fetchedAt: 1, email: null, plan: null, error: null, resetCredits: null, windows: [
    { id: "week", group: "Codex", thirdParty: false, label: "7d", durationSeconds: 604_800, remainingPercent: 80, resetsAt: 2 },
  ] }, error: false });
  render(<ProviderUsageAlertSettings account={{ ...account, usageAlert: { window: "weekly", remainingPercent: 15 } }} saving={false} onChange={vi.fn()} />);
  expect(screen.getByRole("combobox", { name: "Janela" })).toHaveTextContent("Semanal");
  expect(screen.queryByRole("option", { name: "5 horas" })).not.toBeInTheDocument();
});
