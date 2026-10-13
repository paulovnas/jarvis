import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { computerApp, computerSnapshot, computerTarget } from "@/test/computer-fixtures";
import { ComputerUseSession } from "./ComputerUseSession";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
beforeEach(() => { call.mockReset().mockImplementation(async command => command === "list_computer_apps" ? [computerApp] : computerSnapshot()); });

it("keeps YOLO unobtrusive and does not ask for an application grant", async () => {
  const user = userEvent.setup();
  call.mockResolvedValue({ ...computerSnapshot(), mode: "yolo" });
  render(<ComputerUseSession conversationId="new-chat" onStop={vi.fn().mockResolvedValue(undefined)} />);
  await user.click(await screen.findByRole("button", { name: "Usar computador" }));
  expect(await screen.findByText(/sem autorizações por aplicativo ou ação/i)).toBeVisible();
  expect(screen.queryByRole("combobox", { name: "Aplicativo" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Autorizar aplicativo" })).not.toBeInTheDocument();
  expect(call).not.toHaveBeenCalledWith("list_computer_apps", expect.anything());
});

it("opens the exact settings section when the agent reports missing macOS permissions", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  const user = userEvent.setup();
  const snapshot = computerSnapshot({ activity: { operationId: "preflight", targetId: null, tool: "computer_select", phase: "error", message: "Permita Acessibilidade no macOS.", timestamp: 10 } });
  snapshot.capabilities.accessibility = false;
  call.mockResolvedValue(snapshot);
  render(<ComputerUseSession conversationId="new-chat" onStop={vi.fn().mockResolvedValue(undefined)} />);
  expect(await screen.findByText(/Permita Acessibilidade/)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Configurar permissões" }));
  expect(call).toHaveBeenCalledWith("open_auxiliary_window", { kind: "settings", section: "computer" });
  vi.unstubAllGlobals();
});

it("shows the setup shortcut when only screen capture permission is missing", async () => {
  const snapshot = computerSnapshot({ activity: { operationId: "capture-preflight", targetId: null, tool: "computer_observe", phase: "error", message: "Permita Captura de tela no macOS.", timestamp: 10 } });
  snapshot.capabilities.screenCapture = false;
  call.mockResolvedValue(snapshot);
  render(<ComputerUseSession conversationId="new-chat" onStop={vi.fn().mockResolvedValue(undefined)} />);
  expect(await screen.findByRole("button", { name: "Configurar permissões" })).toBeVisible();
  expect(screen.getByText(/Permita Captura de tela/)).toBeVisible();
});

it("authorizes only the chosen application and access in the current conversation after a click", async () => {
  const user = userEvent.setup();
  call.mockImplementation(async command => command === "list_computer_apps" ? [computerApp]
    : command === "authorize_computer_app" ? computerSnapshot({ grants: [{ id: "grant-1", identity: computerApp.identity, access: "observe" }] }) : computerSnapshot());
  render(<ComputerUseSession conversationId="c1" onStop={vi.fn().mockResolvedValue(undefined)} />);
  await user.click(await screen.findByRole("button", { name: "Usar computador" }));
  await user.click(await screen.findByRole("combobox", { name: "Aplicativo" }));
  await user.click(await screen.findByRole("option", { name: "Safari" }));
  expect(call).not.toHaveBeenCalledWith("authorize_computer_app", expect.anything());
  await user.click(screen.getByRole("button", { name: "Autorizar aplicativo" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("authorize_computer_app", { conversationId: "c1", appId: "safari", access: "observe" }));
  expect(await screen.findByRole("button", { name: "Revogar acesso a Safari" })).toBeVisible();
  expect(screen.queryByText("Este projeto")).not.toBeInTheDocument();
});

it("keeps confirmed access visible after a failed revoke and removes it only after native acceptance", async () => {
  const user = userEvent.setup();
  const granted = computerSnapshot({ grants: [{ id: "grant-1", identity: computerApp.identity, access: "control" }], targets: [computerTarget] });
  let fail = true;
  call.mockImplementation(async command => {
    if (command === "list_computer_apps") return [computerApp];
    if (command === "revoke_computer_app") {
      if (fail) throw { message: "A revogação falhou." };
      return computerSnapshot();
    }
    return granted;
  });
  render(<ComputerUseSession conversationId="c1" onStop={vi.fn().mockResolvedValue(undefined)} />);
  await user.click(await screen.findByRole("button", { name: /Usar computador/ }));
  expect(await screen.findByText("Documentação")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Revogar acesso a Safari" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("A revogação falhou.");
  expect(screen.getByRole("button", { name: "Revogar acesso a Safari" })).toBeVisible();
  fail = false;
  await user.click(screen.getByRole("button", { name: "Revogar acesso a Safari" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Revogar acesso a Safari" })).not.toBeInTheDocument());
});

it("displays actual activity and stops native operations together with the agent turn", async () => {
  const user = userEvent.setup();
  const onStop = vi.fn().mockResolvedValue(undefined);
  call.mockImplementation(async command => command === "stop_computer_use" ? computerSnapshot({ stopped: true })
    : computerSnapshot({ grants: [{ id: "grant-1", identity: computerApp.identity, access: "control" }], targets: [computerTarget], activity: { operationId: "op-1", targetId: computerTarget.id, tool: "computer_click", phase: "inputSent", message: "Clique enviado ao Safari.", timestamp: 10 } }));
  render(<ComputerUseSession conversationId="c1" onStop={onStop} />);
  await user.click(await screen.findByRole("button", { name: "Usar computador" }));
  expect(await screen.findByText("Clique enviado ao Safari.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Interromper uso do computador" }));
  expect(call).toHaveBeenCalledWith("stop_computer_use", { conversationId: "c1" });
  expect(onStop).toHaveBeenCalledTimes(1);
  expect(await screen.findByText("Uso do computador interrompido")).toBeVisible();
});
