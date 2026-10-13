import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { computerSnapshot } from "@/test/computer-fixtures";
import { ComputerUseSettings } from "./ComputerUseSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
beforeEach(() => { call.mockReset().mockResolvedValue(computerSnapshot()); });

it("persists the chosen computer mode and explains YOLO without per-app or per-action approval", async () => {
  const user = userEvent.setup();
  call.mockImplementation(async command => command === "set_computer_use_mode"
    ? { ...computerSnapshot(), mode: "yolo" } : { ...computerSnapshot(), mode: "manual" });
  render(<ComputerUseSettings />);
  await user.click(await screen.findByRole("combobox", { name: "Modo de uso do computador" }));
  await user.click(await screen.findByRole("option", { name: "YOLO" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("set_computer_use_mode", { mode: "yolo" }));
  expect(await screen.findByText(/sem autorizações por aplicativo ou ação/i)).toBeVisible();
});

it("shows absent permissions and requests each macOS permission only after a user action", async () => {
  const user = userEvent.setup();
  const status = computerSnapshot();
  status.capabilities.accessibility = false;
  status.capabilities.screenCapture = false;
  status.capabilities.inputPosting = false;
  call.mockImplementation(async command => command === "request_computer_permission" ? status.capabilities : status);
  render(<ComputerUseSettings />);
  expect(await screen.findByRole("button", { name: "Permitir acessibilidade" })).toBeEnabled();
  expect(screen.getAllByText("Não concedida")).toHaveLength(3);
  expect(call).not.toHaveBeenCalledWith("request_computer_permission", expect.anything());
  expect(call).not.toHaveBeenCalledWith("list_computer_apps", expect.anything());
  await user.click(screen.getByRole("button", { name: "Permitir acessibilidade" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("request_computer_permission", { permission: "accessibility" }));
  await user.click(screen.getByRole("button", { name: "Permitir captura de tela" }));
  expect(call).toHaveBeenCalledWith("request_computer_permission", { permission: "screenCapture" });
  await user.click(screen.getByRole("button", { name: "Permitir envio de entradas" }));
  expect(call).toHaveBeenCalledWith("request_computer_permission", { permission: "inputPosting" });
});

it("reports the actual unsupported platform without offering a permission prompt", async () => {
  const status = computerSnapshot();
  status.capabilities = { ...status.capabilities, platform: "linux", supported: false, captureSupported: false, sessionStatus: "unsupported", reason: "O uso nativo do computador está disponível apenas no macOS." };
  call.mockResolvedValue(status);
  render(<ComputerUseSettings />);
  expect(await screen.findByText("O uso nativo do computador está disponível apenas no macOS.")).toBeVisible();
  expect(screen.queryByRole("button", { name: /Permitir/ })).not.toBeInTheDocument();
});

it("shows native failures and can reload the permission state", async () => {
  const user = userEvent.setup();
  call.mockRejectedValueOnce({ code: "computer_unavailable", message: "A sessão gráfica está indisponível." });
  render(<ComputerUseSettings />);
  expect(await screen.findByRole("alert")).toHaveTextContent("A sessão gráfica está indisponível.");
  await user.click(screen.getByRole("button", { name: "Atualizar permissões" }));
  expect(await screen.findAllByText("Concedida")).toHaveLength(3);
});
