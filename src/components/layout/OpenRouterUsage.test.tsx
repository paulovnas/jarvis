import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderAccount } from "@/core/provider-accounts";
import type { AccountUsage } from "@/core/provider-usage";
import { ProviderUsage } from "./ProviderUsage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const account: ProviderAccount = { alias: "openrouter-pessoal", displayName: "Router pessoal", providerKind: "openrouter", enabled: true, createdAt: 1, email: null, accountType: "unknown", models: [], modelsAvailable: true };
const report = (): AccountUsage => ({ alias: account.alias, fetchedAt: Date.now(), email: null, plan: null, error: null, resetCredits: null, windows: [], creditBalance: 7.25, openrouter: { accountBalance: 12.5, keyLimitRemaining: 7.25, note: null } });

beforeEach(() => vi.mocked(invoke).mockReset().mockResolvedValue(report()));

it.each(["compact", "details"] as const)("mostra o saldo em USD e distingue conta e limite da chave no modo %s", async presentation => {
  const user = userEvent.setup();
  render(<ProviderUsage account={account} now={Date.now()} presentation={presentation} />);
  if (presentation === "compact") {
    const button = screen.getByRole("button", { name: "Limites de Router pessoal" });
    await waitFor(() => expect(button).toHaveTextContent("US$ 7,25"));
    expect(button).not.toHaveTextContent(/12,50|Hypercredits|%/);
    await user.click(button);
  }
  const details = screen.getByRole(presentation === "compact" ? "dialog" : "region", { name: "Limites de Router pessoal" });
  await within(details).findByText("Disponível para esta chave");
  expect(within(details).getByText("Disponível para esta chave").closest("dl")).toHaveTextContent("US$ 7,25");
  expect(within(details).getByText("Saldo da conta").closest("dl")).toHaveTextContent("US$ 12,50");
  expect(within(details).getByText("Limite de gasto restante da chave").closest("dl")).toHaveTextContent("US$ 7,25");
  expect(within(details).queryByText(/Hypercredits|Nenhuma janela informada/)).not.toBeInTheDocument();
  expect(within(details).queryByRole("progressbar")).not.toBeInTheDocument();
});

it("mantém saldo desconhecido explícito sem converter o limite de gasto da chave em créditos", async () => {
  const user = userEvent.setup();
  const note = "Saldo da conta indisponível. O limite da chave não representa créditos na conta.";
  vi.mocked(invoke).mockResolvedValue({ ...report(), creditBalance: null, openrouter: { accountBalance: null, keyLimitRemaining: 20.5, note } } satisfies AccountUsage);
  render(<ProviderUsage account={account} now={Date.now()} />);
  const button = screen.getByRole("button", { name: "Limites de Router pessoal" });
  await waitFor(() => expect(button).toHaveTextContent("—"));
  expect(button).not.toHaveTextContent(/US\$|0,00|20,50|Hypercredits/);
  await user.click(button);
  const details = screen.getByRole("dialog", { name: "Limites de Router pessoal" });
  expect(within(details).getByText(note)).toBeVisible();
  expect(within(details).getByText("Limite de gasto restante da chave").closest("dl")).toHaveTextContent("US$ 20,50");
  expect(within(details).queryByText("Saldo da conta")).not.toBeInTheDocument();
  expect(within(details).queryByText("Disponível para esta chave")).not.toBeInTheDocument();
  expect(within(details).queryByText(/US\$\s*0,00|Hypercredits|Nenhuma janela informada/)).not.toBeInTheDocument();
});

it.each([
  { creditBalance: 0, amount: "US$ 0,00" },
  { creditBalance: -0.5, amount: "-US$ 0,50" },
])("preserva o saldo real $creditBalance sem substituir valores por ausência", async ({ creditBalance, amount }) => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValue({ ...report(), creditBalance, openrouter: { accountBalance: creditBalance, keyLimitRemaining: 3.25, note: null } } satisfies AccountUsage);
  render(<ProviderUsage account={account} now={Date.now()} />);
  const button = screen.getByRole("button", { name: "Limites de Router pessoal" });
  await waitFor(() => expect(button).toHaveTextContent(amount));
  expect(button).not.toHaveTextContent(/—|Hypercredits/);
  await user.click(button);
  const details = screen.getByRole("dialog", { name: "Limites de Router pessoal" });
  expect(within(details).getByText("Disponível para esta chave").closest("dl")).toHaveTextContent(amount);
  expect(within(details).getByText("Saldo da conta").closest("dl")).toHaveTextContent(amount);
  expect(within(details).getByText("Limite de gasto restante da chave").closest("dl")).toHaveTextContent("US$ 3,25");
});
