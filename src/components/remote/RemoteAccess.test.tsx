import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { RemoteStatus } from "@/core/remote-control";
import { RemoteAccess } from "./RemoteAccess";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), write: vi.fn(), changed: null as null | ((event: { payload: unknown }) => void) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (_name: string, handler: (event: { payload: unknown }) => void) => { mocks.changed = handler; return () => {}; }) }));
vi.mock("@/core/clipboard", () => ({ writeClipboardText: mocks.write }));
vi.mock("qrcode.react", () => ({ QRCodeSVG: ({ value, title }: { value: string; title: string }) => <svg aria-label={title} data-value={value} /> }));

const disabled: RemoteStatus = { enabled: false, running: false, port: null, urls: [], pairingUrl: null, pairingExpiresAt: null, devices: [], error: null };
const active: RemoteStatus = { enabled: true, running: true, port: 47731, urls: ["http://192.168.1.2:47731/"], pairingUrl: "http://192.168.1.2:47731/#pair=temporary", pairingExpiresAt: 1_800_000_000_000, devices: [], error: null };

beforeEach(() => {
  mocks.invoke.mockReset(); mocks.write.mockReset(); mocks.changed = null;
  mocks.invoke.mockResolvedValue(disabled); mocks.write.mockResolvedValue(undefined);
});

async function open() {
  const user = userEvent.setup();
  render(<RemoteAccess />);
  await user.click(screen.getByRole("button", { name: "Modo remoto" }));
  return user;
}

it("starts disabled and enables the LAN service only from the switch", async () => {
  const user = await open();
  const toggle = await screen.findByRole("switch", { name: "Acesso pela rede local" });
  expect(toggle).not.toBeChecked();
  expect(mocks.invoke).not.toHaveBeenCalledWith("set_remote_enabled", expect.anything());
  mocks.invoke.mockImplementation(async (command: string) => command === "set_remote_enabled" ? active : disabled);
  await user.click(toggle);
  await screen.findByLabelText("QRCode para parear o celular ao Jarvis");
  expect(mocks.invoke).toHaveBeenCalledWith("set_remote_enabled", { enabled: true });
  expect(screen.getByRole("switch")).toBeChecked();
  expect(screen.getByText("Serviço ativo")).toBeVisible();
});

it("copies the pairing link and independently revokes a named device", async () => {
  const connected = { ...active, devices: [{ id: "phone", name: "Meu celular", connectedAt: 123, lastSeenAt: 124 }] };
  mocks.invoke.mockResolvedValue(connected);
  const user = await open();
  await screen.findByText("Meu celular");
  await user.click(screen.getByRole("button", { name: "Copiar link" }));
  expect(mocks.write).toHaveBeenCalledWith(active.pairingUrl);
  mocks.invoke.mockImplementation(async (command: string) => command === "revoke_remote_device" ? active : connected);
  await user.click(screen.getByRole("button", { name: "Revogar Meu celular" }));
  await screen.findByText("Nenhum aparelho conectado.");
  expect(mocks.invoke).toHaveBeenCalledWith("revoke_remote_device", { deviceId: "phone" });
});

it("shows service failures and allows an explicit status refresh", async () => {
  mocks.invoke.mockRejectedValue({ code: "remote_status", message: "Rede indisponível" });
  const user = await open();
  expect(await screen.findByRole("alert")).toHaveTextContent("Rede indisponível");
  mocks.invoke.mockResolvedValue(active);
  await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
  expect(await screen.findByRole("switch")).toBeChecked();
});

it("keeps the newest event when an older status request finishes later", async () => {
  let resolve: ((value: RemoteStatus) => void) | undefined;
  mocks.invoke.mockImplementation(() => new Promise<RemoteStatus>(done => { resolve = done; }));
  await open();
  expect(await screen.findByLabelText("Carregando acesso remoto")).toBeVisible();
  await act(async () => { mocks.changed?.({ payload: active }); });
  expect(screen.getByText("Serviço ativo")).toBeVisible();
  await act(async () => { resolve?.(disabled); });
  await waitFor(() => expect(screen.getByRole("switch")).toBeChecked());
});
