import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { RemoteUsage, type RemoteUsageAccount } from "./RemoteUsage";

afterEach(() => vi.restoreAllMocks());
const now = 1_800_000_000_000;
function account(overrides: Partial<RemoteUsageAccount> = {}): RemoteUsageAccount {
  return { alias: "openai-codex-work", providerKind: "openai-codex", fetchedAt: now, email: null, plan: "pro", error: null, resetCredits: null, windows: [
    { id: "five", group: "Codex", thirdParty: false, label: "5h", durationSeconds: 18_000, remainingPercent: 70, resetsAt: now + 9_000_000 },
    { id: "weekly", group: "Codex", thirdParty: false, label: "Semanal", durationSeconds: 604_800, remainingPercent: 20, resetsAt: now + 302_400_000 },
  ], ...overrides };
}

it("identifies Go and displays monthly quota without inventing reserve or a fixed duration", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const user = userEvent.setup();
  const go = account({ alias: "opencode-go-pessoal", providerKind: "opencode-go", plan: "go", windows: [{ id: "monthly", group: "OpenCode Go", thirdParty: false, label: "Mensal", durationSeconds: null, remainingPercent: 67, resetsAt: now + 86_400_000 }] });
  render(<RemoteUsage load={vi.fn().mockResolvedValue([go])} />);
  await user.click(screen.getByRole("button", { name: "Limites dos provedores" }));
  const region = await screen.findByRole("region", { name: `Limites de ${go.alias}` });
  expect(within(region).getByRole("img", { name: "OpenCode Go" })).toBeVisible();
  expect(within(region).getByText("pessoal")).toBeVisible();
  expect(within(region).getByRole("progressbar", { name: "OpenCode Go Mensal restante" })).toHaveAttribute("aria-valuenow", "67");
  expect(within(region).queryByText(/reserva|déficit/)).not.toBeInTheDocument();
});

it("opens compact provider limits on demand and includes identity, windows, renewals and reserve or deficit", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const user = userEvent.setup();
  const load = vi.fn().mockResolvedValue([account({ resetCredits: { availableCount: 2, expirations: [], detailsAvailable: false } }), account({ alias: "Claude Code", providerKind: "claude-code", plan: null, windows: [], fetchedAt: null, error: "Runtime did not report usage" })]);
  render(<RemoteUsage load={load} />);
  expect(load).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Limites dos provedores" }));
  expect(load).toHaveBeenCalledWith(true);
  const panel = await screen.findByRole("dialog", { name: "Limites dos provedores" });
  const usage = await within(panel).findByRole("region", { name: "Limites de openai-codex-work" });
  expect(within(usage).getByText("work")).toBeVisible();
  expect(within(usage).getByRole("img", { name: "OpenAI Codex" })).toBeVisible();
  expect(within(usage).getByText("Pro")).toBeVisible();
  expect(within(usage).getByRole("progressbar", { name: "Codex 5h restante" })).toHaveAttribute("aria-valuenow", "70");
  expect(within(usage).getByText("Renova em 2h 30m")).toBeVisible();
  expect(within(usage).getByText("20% em reserva")).toBeVisible();
  expect(within(usage).getByText("30% em déficit")).toBeVisible();
  expect(within(usage).getByText("Resets disponíveis")).toBeVisible();
  expect(within(usage).getByText("2")).toBeVisible();
  expect(within(panel).getByRole("img", { name: "Claude Code" })).toBeVisible();
  expect(within(panel).getByText("Limites indisponíveis")).toBeVisible();
});

it("shows a structural loading state and explicitly refreshes the native usage snapshot", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const user = userEvent.setup();
  let resolve: (value: RemoteUsageAccount[]) => void = () => {};
  const load = vi.fn().mockImplementationOnce(() => new Promise<RemoteUsageAccount[]>(done => { resolve = done; })).mockResolvedValue([account({ windows: [{ ...account().windows[0], remainingPercent: 15 }] })]);
  render(<RemoteUsage load={load} />);
  await user.click(screen.getByRole("button", { name: "Limites dos provedores" }));
  expect(screen.getByRole("status", { name: "Carregando limites" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Atualizar limites" })).toBeDisabled();
  await act(async () => resolve([account()]));
  expect(await screen.findByText("70%")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Atualizar limites" }));
  expect(load).toHaveBeenLastCalledWith(true);
  expect(await screen.findByText("15%")).toBeVisible();
  expect(screen.queryByText("70%")).not.toBeInTheDocument();
});

it("keeps last reported quotas after a failed refresh without presenting a stale reserve", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const user = userEvent.setup();
  const load = vi.fn().mockResolvedValueOnce([account()]).mockRejectedValueOnce(new Error("Offline"));
  render(<RemoteUsage load={load} />);
  await user.click(screen.getByRole("button", { name: "Limites dos provedores" }));
  await screen.findByText("20% em reserva");
  await user.click(screen.getByRole("button", { name: "Atualizar limites" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Os últimos dados disponíveis foram preservados.");
  expect(screen.getByText("70%")).toBeVisible();
  expect(screen.queryByText("20% em reserva")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Atualizar limites" })).toBeEnabled();
});

it("distinguishes an empty report, an unavailable window and an outdated snapshot from zero usage", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const user = userEvent.setup();
  const load = vi.fn().mockResolvedValueOnce([]).mockResolvedValueOnce([account({ fetchedAt: now - 360_000, windows: [account().windows[0], { ...account().windows[0], id: "unknown", label: "Extra", remainingPercent: null, resetsAt: null }] })]);
  render(<RemoteUsage load={load} />);
  await user.click(screen.getByRole("button", { name: "Limites dos provedores" }));
  expect(await screen.findByText("Nenhum limite disponível")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Atualizar limites" }));
  expect(await screen.findByText("—")).toBeVisible();
  expect(screen.getByRole("progressbar", { name: "Codex 5h restante" })).toHaveAttribute("aria-valuenow", "70");
  expect(screen.queryByRole("progressbar", { name: "Codex Extra restante" })).not.toBeInTheDocument();
  expect(screen.queryByText(/em reserva|em déficit/)).not.toBeInTheDocument();
});

it("ignores a closed panel response when a newly opened panel has fresher usage", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const user = userEvent.setup();
  let resolve: (value: RemoteUsageAccount[]) => void = () => {};
  const load = vi.fn().mockImplementationOnce(() => new Promise<RemoteUsageAccount[]>(done => { resolve = done; })).mockResolvedValue([account({ windows: [{ ...account().windows[0], remainingPercent: 45 }] })]);
  render(<RemoteUsage load={load} />);
  await user.click(screen.getByRole("button", { name: "Limites dos provedores" }));
  await user.click(screen.getByRole("button", { name: "Fechar limites" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await user.click(screen.getByRole("button", { name: "Limites dos provedores" }));
  expect(await screen.findByText("45%")).toBeVisible();
  await act(async () => resolve([account()]));
  expect(screen.getByText("45%")).toBeVisible();
  expect(screen.queryByText("70%")).not.toBeInTheDocument();
});
