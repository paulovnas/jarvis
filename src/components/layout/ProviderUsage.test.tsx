import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderAccount } from "@/core/provider-accounts";
import type { AccountUsage } from "@/core/provider-usage";
import type { BootstrapResources } from "@/core/bootstrap";
import { BootstrapResourcesProvider } from "@/components/bootstrap/BootstrapResourcesProvider";
import { emptyLibrary } from "@/test/library-fixtures";
import { coreFixture } from "@/test/core-fixtures";
import { StatusBar } from "./StatusBar";
import { ProviderUsage, WindowBar } from "./ProviderUsage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../remote/RemoteAccess", () => ({ RemoteAccess: () => null }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => ({ data: null }) }));
const call = vi.mocked(invoke);
const account: ProviderAccount = { alias: "openai-codex-paulo", providerKind: "openai-codex", enabled: true, createdAt: 1, email: "paulo@example.test", accountType: "personal", models: [], modelsAvailable: true };
const report = (alias: string): AccountUsage => ({ alias, fetchedAt: Date.now(), email: account.email, plan: "pro", error: null, resetCredits: { availableCount: 1, expirations: [Date.now() + 86400_000], detailsAvailable: true }, windows: [{ id: "weekly", group: "Codex", thirdParty: false, label: "7d", durationSeconds: 604800, remainingPercent: 36, resetsAt: Date.now() + 60_000 }] });
beforeEach(() => { call.mockReset().mockImplementation((_command, args) => Promise.resolve(report((args as { alias: string }).alias))); });

it.each([0, 173.5])("shows %s Hypercredits separately without inventing quota windows", async creditBalance => {
  const user = userEvent.setup();
  const hyper = { ...account, alias: "charm-hyper-pessoal", displayName: "Meu Hyper", providerKind: "charm-hyper" };
  call.mockResolvedValue({ ...report(hyper.alias), email: null, plan: null, creditBalance, windows: [], resetCredits: null });
  render(<ProviderUsage account={hyper} now={Date.now()} />);
  const button = screen.getByRole("button", { name: "Limites de Meu Hyper" });
  await waitFor(() => expect(button).toHaveTextContent(`${creditBalance.toLocaleString("pt-BR")} Hypercredits`));
  expect(button).not.toHaveTextContent("%");
  expect(call).toHaveBeenCalledWith("get_provider_usage", { alias: hyper.alias });
  await user.click(button);
  const details = await screen.findByRole("dialog", { name: "Limites de Meu Hyper" });
  expect(within(details).getByText("Charm Hyper · Meu Hyper")).toBeVisible();
  expect(within(details).getByText("Hypercredits")).toBeVisible();
  expect(within(details).queryByRole("progressbar")).not.toBeInTheDocument();
  expect(within(details).queryByText("Nenhuma janela informada.")).not.toBeInTheDocument();
  expect(within(details).queryByText(/em reserva|em déficit/)).not.toBeInTheDocument();
});

it("shows rolling Hyper capacity recovery without calling it a renewal or pacing it", () => {
  render(<WindowBar window={{ id: "five_hour", group: "Charm Hyper", thirdParty: false, label: "5h", durationSeconds: 18_000, remainingPercent: 40, resetsAt: null, nextRecoveryAt: 9_000_000 }} now={0} stale={false} />);
  expect(screen.getByRole("progressbar", { name: "Charm Hyper 5h restante" })).toHaveAttribute("aria-valuenow", "40");
  expect(screen.getByText("Capacidade volta em 2h 30m")).toBeVisible();
  expect(screen.queryByText(/Renova|Reset|em reserva|em déficit|No ritmo/)).not.toBeInTheDocument();
  expect(screen.queryByRole("img", { name: /Restante esperado/ })).not.toBeInTheDocument();
});

it("uses the display name in quota labels while requesting the stable account alias", async () => {
  const user = userEvent.setup();
  const selected = { ...account, displayName: "Conta principal" };
  render(<ProviderUsage account={selected} now={Date.now()} />);
  const button = await screen.findByRole("button", { name: "Limites de Conta principal" });
  await waitFor(() => expect(button).toHaveTextContent("36%"));
  expect(button).toHaveTextContent("Conta principal");
  expect(button).not.toHaveTextContent(account.alias);
  expect(call).toHaveBeenCalledWith("get_provider_usage", { alias: account.alias });
  await user.click(button);
  const details = await screen.findByRole("dialog", { name: "Limites de Conta principal" });
  expect(within(details).getByText("OpenAI Codex · Conta principal")).toBeVisible();
});

it("keeps Go five-hour and weekly quotas in the statusbar and its monthly quota in the details", async () => {
  const user = userEvent.setup();
  const go = { ...account, alias: "opencode-go-pessoal", providerKind: "opencode-go" };
  call.mockResolvedValue({ ...report(go.alias), plan: "go", resetCredits: null, windows: [
    { id: "five_hour", group: "OpenCode Go", thirdParty: false, label: "5h", durationSeconds: 18_000, remainingPercent: 88, resetsAt: Date.now() + 9_000_000 },
    { id: "weekly", group: "OpenCode Go", thirdParty: false, label: "Semanal", durationSeconds: 604_800, remainingPercent: 66, resetsAt: Date.now() + 302_400_000 },
    { id: "monthly", group: "OpenCode Go", thirdParty: false, label: "Mensal", durationSeconds: null, remainingPercent: 67, resetsAt: Date.now() + (18 * 24 + 10) * 3600_000 },
  ] });
  render(<StatusBar accounts={[go]} />);
  const button = await screen.findByRole("button", { name: `Limites de ${go.alias}` });
  await waitFor(() => expect(button).toHaveTextContent("88%"));
  expect(button).toHaveTextContent("pessoal");
  expect(button).toHaveTextContent("5h88%");
  expect(button).toHaveTextContent("Semanal66%");
  expect(button).not.toHaveTextContent("Mensal");
  expect(button).not.toHaveTextContent("67%");
  await user.click(button);
  const details = await screen.findByRole("dialog", { name: `Limites de ${go.alias}` });
  expect(within(details).getByText(`OpenCode Go · ${go.alias}`)).toBeVisible();
  expect(within(details).getByRole("progressbar", { name: "OpenCode Go 5h restante" })).toHaveAttribute("aria-valuenow", "88");
  expect(within(details).getByRole("progressbar", { name: "OpenCode Go Semanal restante" })).toHaveAttribute("aria-valuenow", "66");
  expect(within(details).getByRole("progressbar", { name: "OpenCode Go Mensal restante" })).toHaveAttribute("aria-valuenow", "67");
  const marker = within(details).getByRole("img", { name: "Restante esperado: 61%" });
  expect(marker).toBeVisible();
  expect(parseFloat(marker.style.left)).toBeCloseTo(61.3889, 2);
  expect(within(details).getByText("6% em reserva")).toBeVisible();
});

it.each([
  { remaining: 33, resetsAt: 15 * 86400_000, stale: false, expected: "17% em déficit" },
  { remaining: 50, resetsAt: 15 * 86400_000, stale: false, expected: "No ritmo da janela" },
  { remaining: 67, resetsAt: null, stale: false, expected: null },
  { remaining: 67, resetsAt: 0, stale: false, expected: null },
  { remaining: 67, resetsAt: -1, stale: false, expected: null },
  { remaining: 67, resetsAt: 15 * 86400_000, stale: true, expected: null },
])("renders the shared monthly bar with safe pace labels: $expected", ({ remaining, resetsAt, stale, expected }) => {
  render(<WindowBar window={{ id: "monthly", group: "OpenCode Go", thirdParty: false, label: "Mensal", durationSeconds: null, remainingPercent: remaining, resetsAt }} now={0} stale={stale} />);
  expect(screen.getByRole("progressbar", { name: "OpenCode Go Mensal restante" })).toHaveAttribute("aria-valuenow", String(remaining));
  if (expected) {
    expect(screen.getByText(expected)).toBeVisible();
    expect(screen.getByRole("img", { name: "Restante esperado: 50%" })).toBeVisible();
  } else {
    expect(screen.queryByText(/% em (reserva|déficit)|No ritmo da janela/)).not.toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /Restante esperado/ })).not.toBeInTheDocument();
  }
});

it.each(["openai-codex", "opencode-go", "charm-hyper", "claude-code"])("keeps hidden windows in the %s popover and supports an icon-only quota summary", async providerKind => {
  const user = userEvent.setup();
  const selected = { ...account, providerKind, showFiveHourUsage: false, showWeeklyUsage: true };
  const group = providerKind === "openai-codex" ? "Codex" : providerKind === "opencode-go" ? "OpenCode Go" : providerKind === "charm-hyper" ? "Charm Hyper" : "Claude Code";
  call.mockImplementation(command => Promise.resolve(command === "get_opencode_go_free_models" ? [] : { ...report(selected.alias), windows: [
    { id: "five_hour", group, thirdParty: false, label: "5h", durationSeconds: 18_000, remainingPercent: 88, resetsAt: Date.now() + 9_000_000 },
    { id: "weekly", group, thirdParty: false, label: "7d", durationSeconds: 604_800, remainingPercent: 66, resetsAt: Date.now() + 302_400_000 },
  ] }));
  const { rerender } = render(<ProviderUsage account={selected} now={Date.now()} />);
  const button = screen.getByRole("button", { name: `Limites de ${selected.alias}` });
  await waitFor(() => expect(button).toHaveTextContent("66%"));
  expect(button).not.toHaveTextContent("5h");
  await user.click(button);
  const details = await screen.findByRole("dialog", { name: `Limites de ${selected.alias}` });
  expect(within(details).getByRole("progressbar", { name: `${group} 5h restante` })).toHaveAttribute("aria-valuenow", "88");
  rerender(<ProviderUsage account={{ ...selected, showFiveHourUsage: true, showWeeklyUsage: false }} now={Date.now()} />);
  expect(button).toHaveTextContent("88%");
  expect(button).not.toHaveTextContent("7d");
  rerender(<ProviderUsage account={{ ...selected, showWeeklyUsage: false }} now={Date.now()} />);
  expect(button).not.toHaveTextContent("88%");
  expect(button).not.toHaveTextContent("66%");
  expect(within(details).getByRole("progressbar", { name: `${group} 7d restante` })).toHaveAttribute("aria-valuenow", "66");
});

it("loads verified Go freebies only when opening its popover, independently of failed quotas", async () => {
  const user = userEvent.setup();
  const go = { ...account, alias: "opencode-go-pessoal", providerKind: "opencode-go" };
  call.mockImplementation(command => command === "get_opencode_go_free_models" ? Promise.resolve([
    { id: "space-bunny-free", name: "Space Bunny Free" },
    { id: "longcat-2.5-preview-free", name: "LongCat 2.5 Preview Free" },
  ]) : Promise.reject(new Error("Quota unavailable")));
  render(<StatusBar accounts={[go]} />);
  const button = await screen.findByRole("button", { name: `Limites de ${go.alias}` });
  await screen.findByLabelText("Limites desatualizados");
  expect(call).not.toHaveBeenCalledWith("get_opencode_go_free_models");
  await user.click(button);
  const section = await screen.findByRole("region", { name: "Modelos gratuitos agora" });
  expect(await within(section).findByText("Space Bunny Free")).toBeVisible();
  expect(within(section).getByText("LongCat 2.5 Preview Free")).toBeVisible();
  expect(within(section).getAllByText("Grátis")).toHaveLength(2);
  expect(screen.getByText("Limites indisponíveis")).toBeVisible();
});

it("shows unavailable free-model pricing without inventing an empty or free catalog", async () => {
  const user = userEvent.setup();
  const go = { ...account, alias: "opencode-go-pessoal", providerKind: "opencode-go" };
  call.mockImplementation(command => command === "get_opencode_go_free_models" ? Promise.reject(new Error("Catalog unavailable")) : Promise.resolve(report(go.alias)));
  render(<StatusBar accounts={[go]} />);
  await user.click(await screen.findByRole("button", { name: `Limites de ${go.alias}` }));
  expect(await screen.findByText("Não foi possível confirmar os gratuitos agora.")).toBeVisible();
  expect(screen.queryByText("Nenhum modelo gratuito no momento.")).not.toBeInTheDocument();
  expect(screen.queryByText("Grátis")).not.toBeInTheDocument();
});

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
  expect(screen.getByLabelText(`Carregando limites de ${account.alias}`)).toBeInTheDocument();
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
  call.mockImplementation(command => Promise.resolve(command === "list_provider_accounts" ? [account] : coreFixture()));
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
  expect(call).not.toHaveBeenCalledWith("get_provider_usage", { alias: account.alias });
});
