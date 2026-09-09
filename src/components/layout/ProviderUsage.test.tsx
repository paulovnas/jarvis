import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderAccount } from "@/core/provider-accounts";
import type { AccountUsage } from "@/core/provider-usage";
import type { BootstrapResources } from "@/core/bootstrap";
import { BootstrapResourcesProvider } from "@/components/bootstrap/BootstrapResourcesProvider";
import { emptyLibrary } from "@/test/library-fixtures";
import { StatusBar } from "./StatusBar";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const account: ProviderAccount = { alias: "openai-codex-paulo", providerKind: "openai-codex", enabled: true, createdAt: 1, email: "paulo@example.test", accountType: "personal", models: [], modelsAvailable: true };
const report = (alias: string): AccountUsage => ({ alias, fetchedAt: Date.now(), email: account.email, plan: "pro", error: null, resetCredits: { availableCount: 1, expirations: [Date.now() + 86400_000], detailsAvailable: true }, windows: [{ id: "weekly", group: "Codex", thirdParty: false, label: "7d", durationSeconds: 604800, remainingPercent: 36, resetsAt: Date.now() + 60_000 }] });
beforeEach(() => { call.mockReset().mockImplementation((_command, args) => Promise.resolve(report((args as { alias: string }).alias))); });

it("shows remaining percentages and details on hover or keyboard without inventing a five-hour window", async () => {
  const user = userEvent.setup();
  const settings = vi.fn();
  render(<StatusBar accounts={[account]} onOpenSettings={settings} />);
  const button = await screen.findByRole("button", { name: `Limites de ${account.alias}` });
  await waitFor(() => expect(button).toHaveTextContent("36%"));
  expect(button).toHaveTextContent("paulo");
  expect(button).not.toHaveTextContent("5h");
  await user.hover(button);
  expect(await screen.findByText(account.email!)).toBeInTheDocument();
  expect(screen.getByRole("progressbar", { name: "Codex 7d restante" })).toHaveAttribute("aria-valuenow", "36");
  expect(screen.getByText("Resets disponíveis")).toBeInTheDocument();
  expect(screen.getByText(/Expira em/)).toBeInTheDocument();
  expect(call.mock.calls.every(([name]) => name === "get_provider_usage")).toBe(true);
  await user.unhover(button);
  await waitFor(() => expect(screen.queryByRole("dialog", { name: `Limites de ${account.alias}` })).not.toBeInTheDocument());
  button.focus();
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("dialog", { name: `Limites de ${account.alias}` })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Configurações" }));
  expect(settings).toHaveBeenCalledOnce();
});

it.each([
  { remaining: 78, days: 3.5, stale: false, expected: "28% em reserva" },
  { remaining: 76, days: 6.5, stale: false, expected: "17% em déficit" },
  { remaining: 78, days: 3.5, stale: true, expected: null },
])("mostra o ritmo da janela apenas para dados atuais: $expected", async ({ remaining, days, stale, expected }) => {
  const data = report(account.alias);
  call.mockResolvedValue({ ...data, error: stale ? "offline" : null, windows: [{ ...data.windows[0], remainingPercent: remaining, resetsAt: Date.now() + days * 86400_000 }] });
  const user = userEvent.setup();
  render(<StatusBar accounts={[account]} />);
  const button = await screen.findByRole("button", { name: `Limites de ${account.alias}` });
  await waitFor(() => expect(button).toHaveTextContent(`${remaining}%`));
  await user.hover(button);
  await screen.findByRole("dialog", { name: `Limites de ${account.alias}` });
  if (expected) {
    expect(screen.getByText(expected)).toBeVisible();
    const marker = screen.getByRole("img", { name: `Restante esperado: ${Math.round(days / 7 * 100)}%` });
    expect(marker).toBeVisible();
    expect(parseFloat(marker.style.left)).toBeCloseTo(days / 7 * 100, 2);
  } else {
    expect(screen.queryByText(/% em (reserva|déficit)/)).not.toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /Restante esperado/ })).not.toBeInTheDocument();
  }
});

it("hides disabled accounts and third-party limits by default, then reflects the preference", async () => {
  const google = { ...account, alias: "antigravity-pessoal", providerKind: "antigravity" };
  call.mockResolvedValue({ ...report(google.alias), resetCredits: null, windows: [
    { ...report(google.alias).windows[0], group: "Gemini" },
    { ...report(google.alias).windows[0], id: "other", group: "Outros", thirdParty: true, remainingPercent: 91 },
  ] });
  const { rerender } = render(<StatusBar accounts={[{ ...account, showUsage: false }, { ...account, alias: "openai-codex-off", enabled: false }, google]} />);
  const button = await screen.findByRole("button", { name: `Limites de ${google.alias}` });
  await waitFor(() => expect(button).toHaveTextContent("36%"));
  expect(button).not.toHaveTextContent("91%");
  expect(call).toHaveBeenCalledTimes(1);
  rerender(<StatusBar accounts={[{ ...google, showThirdPartyUsage: true }]} />);
  expect(button).toHaveTextContent("91%");
  expect(button).toHaveTextContent("3P");
});

it("ignores a late response after hiding an account and isolates failed accounts", async () => {
  let resolve!: (value: AccountUsage) => void;
  call.mockImplementation((_command, args) => (args as { alias: string }).alias === account.alias ? new Promise<AccountUsage>(done => { resolve = done; }) : Promise.reject(new Error("offline")));
  const other = { ...account, alias: "openai-codex-offline" };
  const { rerender } = render(<StatusBar accounts={[account, other]} />);
  expect(await screen.findByLabelText("Limites desatualizados")).toBeInTheDocument();
  expect(screen.getByLabelText("Carregando limites de paulo")).toBeInTheDocument();
  rerender(<StatusBar accounts={[{ ...account, showUsage: false }, other]} />);
  await act(async () => resolve(report(account.alias)));
  expect(screen.queryByRole("button", { name: `Limites de ${account.alias}` })).not.toBeInTheDocument();
  expect(within(screen.getByRole("button", { name: `Limites de ${other.alias}` })).getByLabelText("Limites desatualizados")).toBeInTheDocument();
});

it("keeps polling alerts when an account is hidden from the statusbar", async () => {
  render(<StatusBar accounts={[{ ...account, showUsage: false, usageAlert: { window: "weekly", remainingPercent: 20 } }]} />);
  await waitFor(() => expect(call).toHaveBeenCalledWith("get_provider_usage", { alias: account.alias }));
  expect(screen.queryByRole("button", { name: `Limites de ${account.alias}` })).not.toBeInTheDocument();
});

it("reuses limits fetched during bootstrap instead of requesting them again", async () => {
  const cached = report(account.alias);
  const resources: BootstrapResources = {
    core: null,
    skills: null,
    accounts: [account],
    usageByAlias: { [account.alias]: { data: cached, error: false } },
    library: emptyLibrary(),
    checked: { core: false, skills: false },
    loaded: { core: false, skills: false, accounts: true, usage: true, library: true },
    warnings: [],
  };

  render(<BootstrapResourcesProvider initial={resources}><StatusBar passive accounts={[account]} /></BootstrapResourcesProvider>);

  expect(await screen.findByRole("button", { name: `Limites de ${account.alias}` })).toHaveTextContent("36%");
  expect(call).not.toHaveBeenCalled();
});
