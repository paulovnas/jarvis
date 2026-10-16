import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { beforeEach, expect, it, vi } from "vitest";
import type { MuseRuntime } from "@/core/executors";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const status = (installed = true): MuseRuntime => ({ installed, authenticated: false, version: installed ? "1" : null, error: null, models: [], preferences: { enabled: true, disabledModels: [] } });
const login = { id: "attempt-1", verificationUrl: "https://auth.meta.com/device", userCode: "ABCD-EFGH", expiresAt: Date.now() + 600_000 };
beforeEach(() => { vi.resetModules(); vi.mocked(invoke).mockReset(); vi.mocked(listen).mockClear(); vi.mocked(openUrl).mockReset().mockResolvedValue(); });

it("explains the official CLI installation and subscription requirement without accepting API credentials", async () => {
  const { MuseProviderCard } = await import("./MuseProviderCard");
  vi.mocked(invoke).mockResolvedValue(status(false));
  const user = userEvent.setup();
  render(<MuseProviderCard />);
  await waitFor(() => expect(screen.getByTestId("provider-account-muse-code")).toHaveTextContent("Instalação necessária"));
  await user.click(screen.getByRole("button", { name: "Detalhes de Muse Code" }));
  const dialog = screen.getByRole("dialog", { name: "Muse Code" });
  expect(dialog).toHaveTextContent("assinatura Muse Code");
  expect(dialog).toHaveTextContent("API Meta paga por uso");
  expect(dialog).toHaveTextContent("perfil privado do Jarvis");
  expect(dialog).not.toHaveTextContent("no terminal e atualize o status");
  expect(dialog).toHaveTextContent("Muse Code não encontrado");
  expect(within(dialog).queryByRole("textbox")).not.toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Conectar assinatura Muse Code" })).toBeDisabled();
  await user.click(within(dialog).getByRole("button", { name: "Instalar Muse Code" }));
  expect(openUrl).toHaveBeenCalledWith(expect.stringContaining("muse-code"));
});

it("completes browser login from runtime events and only disconnects on its explicit action", async () => {
  const { MuseProviderCard } = await import("./MuseProviderCard");
  vi.mocked(invoke).mockImplementation(async command => command === "start_muse_login" ? login : status());
  const user = userEvent.setup();
  render(<MuseProviderCard />);
  await user.click(screen.getByRole("button", { name: "Detalhes de Muse Code" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Conectar assinatura Muse Code" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Conectar assinatura Muse Code" }));
  expect(await screen.findByText("ABCD-EFGH")).toBeVisible();
  expect(openUrl).toHaveBeenCalledWith(login.verificationUrl);
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "muse-runtime:changed")?.[1];
  act(() => changed?.({ event: "muse-runtime:changed", id: 1, payload: { ...status(), authenticated: true, email: "muse@example.test", subscriptionType: "pro" } }));
  expect(await screen.findByRole("button", { name: "Desconectar Muse Code" })).toBeVisible();
  expect(screen.queryByText("ABCD-EFGH")).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalledWith("disconnect_muse_account");
  act(() => changed?.({ event: "muse-runtime:changed", id: 2, payload: status() }));
  expect(await screen.findByRole("button", { name: "Conectar assinatura Muse Code" })).toBeVisible();
  expect(invoke).not.toHaveBeenCalledWith("disconnect_muse_account");
  act(() => changed?.({ event: "muse-runtime:changed", id: 3, payload: { ...status(), authenticated: true } }));
  await user.click(await screen.findByRole("button", { name: "Desconectar Muse Code" }));
  expect(invoke).toHaveBeenCalledWith("disconnect_muse_account");
});

it("cancels a pending login even if its start response arrives late without disconnecting the account", async () => {
  const { MuseProviderCard } = await import("./MuseProviderCard");
  let finish: (value: typeof login) => void = () => {};
  const pending = new Promise<typeof login>(resolve => { finish = resolve; });
  vi.mocked(invoke).mockImplementation(command => command === "start_muse_login" ? pending : Promise.resolve(status()));
  const user = userEvent.setup();
  render(<MuseProviderCard />);
  await user.click(screen.getByRole("button", { name: "Detalhes de Muse Code" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Conectar assinatura Muse Code" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Conectar assinatura Muse Code" }));
  await user.click(await screen.findByRole("button", { name: "Cancelar login" }));
  await act(async () => { finish(login); });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("cancel_muse_login", { id: login.id }));
  expect(openUrl).not.toHaveBeenCalled();
  expect(screen.queryByText("ABCD-EFGH")).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalledWith("disconnect_muse_account");
  expect(screen.getByRole("button", { name: "Conectar assinatura Muse Code" })).toBeEnabled();
});
