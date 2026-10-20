import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { ProviderAccount } from "@/core/provider-accounts";
import { OpenCodeGoProviderForm } from "./OpenCodeGoProviderForm";
import { openUrl } from "@tauri-apps/plugin-opener";

const { call } = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: call }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const account: ProviderAccount = { alias: "opencode-go-pessoal", providerKind: "opencode-go", enabled: true, showUsage: true, createdAt: 1, email: null, accountType: "personal", modelsAvailable: true, models: [{ id: "glm", name: "GLM", reasoningLevels: [], defaultReasoningLevel: null }] };
beforeEach(() => { call.mockReset().mockResolvedValue(account); vi.mocked(openUrl).mockReset().mockResolvedValue(undefined); });

it("configures Brave Search without model discovery and preserves the key when editing", async () => {
  const brave: ProviderAccount = { ...account, alias: "brave-search-work", displayName: "Brave Search", providerKind: "brave-search", models: [] };
  call.mockResolvedValue(brave);
  const user = userEvent.setup(), saved = vi.fn();
  const view = render(<OpenCodeGoProviderForm providerKind="brave-search" onSaved={saved} onCancel={vi.fn()} />);
  expect(screen.getByText(/sem consumir cota ao cadastrar/)).toBeVisible();
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "work" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Nome de exibição" }), { target: { value: "Brave Search" } });
  fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: " brave-private-test-key " } });
  await user.click(screen.getByRole("button", { name: "Conectar Brave Search" }));
  await waitFor(() => expect(saved).toHaveBeenCalledWith(brave));
  expect(call).toHaveBeenCalledExactlyOnceWith("save_brave_search_provider", { alias: brave.alias, displayName: "Brave Search", apiKey: "brave-private-test-key", editing: false });
  expect(screen.getByLabelText("Chave de API")).toHaveValue("");
  view.rerender(<OpenCodeGoProviderForm key={brave.alias} account={brave} onSaved={saved} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Salvar alterações" }));
  await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_brave_search_provider", { alias: brave.alias, displayName: "Brave Search", apiKey: null, editing: true }));
  await user.click(screen.getByRole("button", { name: "Abrir Brave Search" }));
  expect(openUrl).toHaveBeenCalledWith("https://api-dashboard.search.brave.com/");
});

it("connects Charm Hyper with its native command and preserves its display name while editing", async () => {
  const hyper: ProviderAccount = { ...account, alias: "charm-hyper-pessoal", providerKind: "charm-hyper", displayName: "Meu Hyper" };
  call.mockImplementation((command: string) => Promise.resolve(command === "get_charm_hyper_panel_status" ? { connected: false, teamName: null, teamId: null, teams: [], flowId: null } : hyper));
  const saved = vi.fn(), user = userEvent.setup();
  const view = render(<OpenCodeGoProviderForm providerKind="charm-hyper" onSaved={saved} onCancel={vi.fn()} />);
  expect(screen.getByText(/A chave de API consulta o catálogo e o saldo em Hypercredits/)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Conectar painel" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Abrir Charm Hyper" }));
  expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://hyper.charm.land/");
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Nome de exibição" }), { target: { value: "Meu Hyper" } });
  fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: " hyper-test-key " } });
  await user.click(screen.getByRole("button", { name: "Conectar Charm Hyper" }));
  await waitFor(() => expect(saved).toHaveBeenCalledWith(hyper));
  expect(call).toHaveBeenCalledExactlyOnceWith("save_charm_hyper_provider", { alias: hyper.alias, displayName: "Meu Hyper", apiKey: "hyper-test-key", editing: false });
  view.rerender(<OpenCodeGoProviderForm key={hyper.alias} providerKind="charm-hyper" account={hyper} onSaved={saved} onCancel={vi.fn()} />);
  expect(await screen.findByRole("button", { name: "Conectar painel" })).toBeEnabled();
  expect(screen.getByRole("textbox", { name: "Nome de exibição" })).toHaveValue("Meu Hyper");
  await user.click(screen.getByRole("button", { name: "Salvar alterações" }));
  await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_charm_hyper_provider", { alias: hyper.alias, displayName: "Meu Hyper", apiKey: null, editing: true }));
});

it.each([undefined, account])("keeps panel connection exclusive to existing Hyper accounts: %j", account => {
  render(<OpenCodeGoProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.queryByRole("button", { name: "Conectar painel" })).not.toBeInTheDocument();
  expect(call).not.toHaveBeenCalled();
});

it("blocks save and cancel only while a panel request is pending", async () => {
  const hyper: ProviderAccount = { ...account, alias: "charm-hyper-pessoal", providerKind: "charm-hyper" };
  const status = { connected: false, teamName: null, teamId: null, teams: [], flowId: null };
  let resolve!: (value: unknown) => void;
  call.mockImplementation((command: string) => command === "start_charm_hyper_panel_connection" ? new Promise(done => { resolve = done; }) : Promise.resolve(command === "get_charm_hyper_panel_status" ? status : hyper));
  const busy = vi.fn(), cancel = vi.fn(), saved = vi.fn(); const user = userEvent.setup();
  const { container } = render(<OpenCodeGoProviderForm account={hyper} onSaved={saved} onCancel={cancel} onBusyChange={busy} />);
  await user.click(await screen.findByRole("button", { name: "Conectar painel" }));
  expect(screen.getByRole("button", { name: "Salvar alterações" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Cancelar" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Abrir Charm Hyper" })).toBeDisabled();
  fireEvent.submit(container.querySelector("form")!);
  expect(call.mock.calls.some(([command]) => command === "save_charm_hyper_provider")).toBe(false);
  expect(busy).toHaveBeenLastCalledWith(true);
  await act(async () => resolve({ ...status, flowId: "flow" }));
  expect(screen.getByRole("button", { name: "Verificar conexão" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Salvar alterações" })).toBeEnabled();
  expect(busy).toHaveBeenLastCalledWith(false);
  await user.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(cancel).toHaveBeenCalledOnce();
  expect(saved).not.toHaveBeenCalled();
});

it("defaults the display name to the complete alias and requires a name of at most 18 characters", async () => {
  const user = userEvent.setup();
  render(<OpenCodeGoProviderForm onSaved={vi.fn()} onCancel={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  const name = screen.getByRole("textbox", { name: "Nome de exibição" });
  expect(name).toHaveValue(account.alias);
  fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: "private-test-key" } });
  fireEvent.change(name, { target: { value: "x".repeat(19) } });
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  expect(screen.getByRole("alert")).toHaveTextContent("18");
  expect(call).not.toHaveBeenCalled();
  fireEvent.change(name, { target: { value: "Minha assinatura" } });
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_opencode_go_provider", { alias: account.alias, displayName: "Minha assinatura", apiKey: "private-test-key", editing: false }));
});

it("connects a subscription by API key and returns discovered models without exposing the key", async () => {
  const saved = vi.fn(); const user = userEvent.setup();
  render(<OpenCodeGoProviderForm onSaved={saved} onCancel={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Nome de exibição" }), { target: { value: "Minha conta" } });
  const key = screen.getByLabelText("Chave de API");
  expect(key).toHaveAttribute("type", "password");
  fireEvent.change(key, { target: { value: " private-test-key " } });
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  await waitFor(() => expect(saved).toHaveBeenCalledWith(account));
  expect(call).toHaveBeenCalledExactlyOnceWith("save_opencode_go_provider", { alias: account.alias, displayName: "Minha conta", apiKey: "private-test-key", editing: false });
  expect(key).toHaveValue("");
  expect(screen.queryByText("private-test-key")).not.toBeInTheDocument();
});

it("requires a valid alias and a key before connecting", async () => {
  const user = userEvent.setup();
  render(<OpenCodeGoProviderForm onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  expect(screen.getByRole("alert")).toHaveTextContent("sufixo");
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Nome de exibição" }), { target: { value: "Minha conta" } });
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  expect(screen.getByRole("alert")).toHaveTextContent("chave de API");
  expect(call).not.toHaveBeenCalled();
});

it.each([null, "replacement-test-key"])("keeps the account identity while replacing or preserving its key: %s", async apiKey => {
  const user = userEvent.setup();
  render(<OpenCodeGoProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.getByLabelText("Sufixo do alias")).toBeDisabled();
  if (apiKey) fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: apiKey } });
  await user.click(screen.getByRole("button", { name: "Salvar alterações" }));
  await waitFor(() => expect(call).toHaveBeenCalledExactlyOnceWith("save_opencode_go_provider", { alias: account.alias, apiKey, editing: true }));
});

it("locks duplicate submissions and keeps a failed connection editable without showing its secret", async () => {
  let reject!: (reason: Error) => void;
  call.mockImplementation(() => new Promise<unknown>((_resolve, rejectPromise) => { reject = rejectPromise; }));
  const busy = vi.fn(); const saved = vi.fn();
  const { container } = render(<OpenCodeGoProviderForm onSaved={saved} onCancel={vi.fn()} onBusyChange={busy} />);
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Nome de exibição" }), { target: { value: "Minha conta" } });
  fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: "private-test-key" } });
  fireEvent.submit(container.querySelector("form")!); fireEvent.submit(container.querySelector("form")!);
  expect(call).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Conectando…" })).toBeDisabled();
  await act(async () => reject(new Error("Chave private-test-key não aceita.")));
  expect(screen.getByRole("alert")).toHaveTextContent("[chave ocultada]");
  expect(screen.getByRole("alert")).not.toHaveTextContent("private-test-key");
  expect(screen.getByRole("button", { name: "Conectar OpenCode Go" })).toBeEnabled();
  expect(busy.mock.calls).toEqual([[true], [false]]);
  expect(saved).not.toHaveBeenCalled();
});
