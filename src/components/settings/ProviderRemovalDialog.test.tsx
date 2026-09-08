import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { providerReference, referenceAccount } from "@/test/provider-reference-fixtures";
import { ProviderRemovalDialog } from "./ProviderRemovalDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const item = providerReference();
const plan = { alias: "antigo", revision: "review-1", items: [item] };
beforeEach(() => call.mockReset().mockImplementation(async command => command === "get_provider_removal_plan" ? plan : { replaced: 0, unresolved: [item] }));
function setup(accounts = [referenceAccount("antigo"), referenceAccount()]) {
  const removed = vi.fn().mockResolvedValue(undefined); const close = vi.fn(); const busy = vi.fn();
  render(<ProviderRemovalDialog alias="antigo" accounts={accounts} onClose={close} onRemoved={removed} onBusyChange={busy} />);
  return { removed, close, busy, user: userEvent.setup() };
}

it("previews dependencies and permits removal with no replacement", async () => {
  const { user, removed } = setup();
  expect(screen.getByRole("button", { name: "Remover provedor" })).toBeDisabled();
  await screen.findByText("Fluxo: Revisão", { exact: false });
  expect(screen.getByText("Recomendamos substituir os modelos vinculados")).toBeVisible();
  expect(screen.getByText("0 de 1 substituições definidas · 1 sem destino")).toBeVisible();
  expect(call).not.toHaveBeenCalledWith("disconnect_provider_account", expect.anything());
  await user.click(screen.getByRole("button", { name: "Remover provedor" }));
  await waitFor(() => expect(removed).toHaveBeenCalledWith({ replaced: 0, unresolved: [item] }));
  expect(call).toHaveBeenCalledWith("disconnect_provider_account", { alias: "antigo", revision: "review-1", replacements: [] });
});

it("sends the reviewed mapping and locks duplicate submissions", async () => {
  const { user, close, removed } = setup();
  await user.click(await screen.findByRole("combobox", { name: "Novo provedor para Analista" }));
  expect(screen.queryByRole("option", { name: "antigo" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("option", { name: "novo" }));
  await user.click(screen.getByRole("combobox", { name: "Raciocínio para Analista" }));
  await user.click(screen.getByRole("option", { name: "Alto" }));
  expect(screen.getByText("1 de 1 substituições definidas")).toBeVisible();
  let finish!: (value: unknown) => void;
  call.mockImplementation(command => command === "disconnect_provider_account" ? new Promise(resolve => { finish = resolve; }) : Promise.resolve(plan));
  await user.dblClick(screen.getByRole("button", { name: "Substituir e remover" }));
  expect(call.mock.calls.filter(([command]) => command === "disconnect_provider_account")).toHaveLength(1);
  expect(call).toHaveBeenCalledWith("disconnect_provider_account", { alias: "antigo", revision: "review-1", replacements: [{ id: item.id, choice: { account: "novo", model: "gpt-test", reasoning: "high" } }] });
  expect(screen.getByRole("button", { name: "Cancelar" })).toBeDisabled();
  await user.keyboard("{Escape}"); expect(close).not.toHaveBeenCalled();
  finish({ replaced: 1, unresolved: [] });
  await waitFor(() => expect(removed).toHaveBeenCalledWith({ replaced: 1, unresolved: [] }));
});

it("keeps the provider and selections on failure and requires a fresh review after concurrent changes", async () => {
  const error = vi.spyOn(toast, "error"); const { user, removed } = setup();
  await screen.findByRole("combobox", { name: "Novo provedor para Analista" });
  call.mockRejectedValueOnce({ code: "provider_links_changed", message: "Os vínculos mudaram." });
  await user.click(screen.getByRole("button", { name: "Remover provedor" }));
  expect(await screen.findByText("Os vínculos mudaram.")).toBeVisible();
  expect(error).toHaveBeenCalledWith("Os vínculos mudaram.");
  expect(removed).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Remover provedor" })).toBeDisabled();
  call.mockResolvedValueOnce({ ...plan, revision: "review-2", items: [] });
  await user.click(screen.getByRole("button", { name: "Recarregar vínculos" }));
  await screen.findByText("Nenhum item configurado está vinculado a este provedor.");
  await user.click(screen.getByRole("button", { name: "Remover provedor" }));
  expect(call).toHaveBeenLastCalledWith("disconnect_provider_account", { alias: "antigo", revision: "review-2", replacements: [] });
});

it("explains when the last compatible provider is removed and cancellation has no side effects", async () => {
  const { user, close } = setup([referenceAccount("antigo")]);
  const row = await screen.findByLabelText("Vínculo: Analista");
  expect(within(row).getByText(/Nenhum outro provedor compatível/)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(close).toHaveBeenCalledOnce();
  expect(call).not.toHaveBeenCalledWith("disconnect_provider_account", expect.anything());
});
