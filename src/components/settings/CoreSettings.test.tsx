import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { coreFixture } from "@/test/core-fixtures";
import { CoreSettings } from "./CoreSettings";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const events = new Map<string, EventCallback<unknown>>();
beforeEach(() => {
  invokeMock.mockReset(); events.clear();
  vi.mocked(listen).mockImplementation(async (name, callback) => { events.set(name, callback); return () => { events.delete(name); }; });
});

it.each([0, 1, 2, 3, 4, 5])("updates real download progress for Core item %i and resets it between stages", async index => {
  const state = coreFixture(false);
  state.items[index].stage = "Baixando recursos";
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  const bar = await screen.findByRole("progressbar", { name: `Instalação de ${state.items[index].name}` });
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  expect(bar).not.toHaveAttribute("aria-valuenow");
  expect(bar).toHaveAttribute("data-indeterminate");
  await act(async () => events.get("core:download")?.({ event: "core:download", id: 1, payload: { id: state.items[index].id, download: { receivedBytes: 1048576, totalBytes: 4194304 } } }));
  expect(bar).toHaveAttribute("aria-valuenow", "25");
  expect(screen.getByText("25%")).toBeInTheDocument();
  expect(screen.getByText("1 MB / 4 MB")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: `Instalar ${state.items[index].name}` })).toBeDisabled();
  state.items[index].stage = "Extraindo arquivos";
  await act(async () => events.get("core:changed")?.({ event: "core:changed", id: 2, payload: state }));
  expect(bar).not.toHaveAttribute("aria-valuenow");
  expect(screen.queryByText("25%")).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("Extraindo arquivos");
  state.items[index].stage = null;
  state.items[index].error = "Download interrompido";
  await act(async () => events.get("core:changed")?.({ event: "core:changed", id: 3, payload: state }));
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  expect(screen.getByRole("alert")).toHaveTextContent("Download interrompido");
  expect(screen.getByRole("button", { name: `Instalar ${state.items[index].name}` })).toBeEnabled();
});

it("restores an ongoing download without inventing a percentage when the server omits its size", async () => {
  const state = coreFixture(false);
  state.items[3].stage = "Baixando recursos de design";
  state.items[3].download = { receivedBytes: 12582912, totalBytes: null };
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  const bar = await screen.findByRole("progressbar", { name: "Instalação de Open Design" });
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  expect(bar).not.toHaveAttribute("aria-valuenow");
  expect(bar).toHaveAttribute("data-indeterminate");
  expect(screen.getByText("12 MB")).toBeInTheDocument();
  await act(async () => events.get("core:download")?.({ event: "core:download", id: 1, payload: { id: "open-design", download: { receivedBytes: 16777216, totalBytes: null } } }));
  expect(screen.getByText("16 MB")).toBeInTheDocument();
});

it("preserves newer download progress when an earlier update check finishes", async () => {
  const state = coreFixture(false);
  state.items[3].stage = "Baixando recursos de design";
  state.items[3].download = { receivedBytes: 12582912, totalBytes: null };
  let finishCheck!: (value: unknown) => void;
  invokeMock.mockImplementation(async command => command === "check_core_updates"
    ? new Promise(resolve => { finishCheck = resolve; })
    : state);
  render(<CoreSettings />);
  await screen.findByText("12 MB");
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_core_updates"));
  await act(async () => events.get("core:download")?.({ event: "core:download", id: 1, payload: { id: "open-design", download: { receivedBytes: 16777216, totalBytes: null } } }));
  expect(screen.getByText("16 MB")).toBeInTheDocument();
  await act(async () => finishCheck(state));
  expect(screen.getByText("16 MB")).toBeInTheDocument();
  expect(screen.queryByText("12 MB")).not.toBeInTheDocument();
});

it("explains the longer Open Design installation through its help tooltip", async () => {
  const user = userEvent.setup(); invokeMock.mockResolvedValue(coreFixture());
  render(<CoreSettings />);
  await user.hover(await screen.findByRole("button", { name: "Sobre a instalação do Open Design" }));
  expect(await screen.findByText(/O download e a preparação podem levar alguns minutos/)).toBeVisible();
});

it("offers Open Design installation alongside the other Core resources", async () => {
  const state = coreFixture(); state.ready = false; state.items[3].installed = false; state.items[3].installedVersion = null;
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: "Instalar Open Design" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "open-design" }));
  expect(screen.getByText("5/6")).toBeInTheDocument();
});

it("mostra versões e só oferece atualização quando há release maior", async () => {
  const state = coreFixture(); state.items[0].latestVersion = "1.1.0"; state.items[0].updateAvailable = true;
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);
  expect(await screen.findByRole("button", { name: "Atualizar Context-mode" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Atualizar Ponytail" })).not.toBeInTheDocument();
  expect(screen.getByText("→ 1.1.0")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Atualizar Context-mode" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "context-mode" }));
});

it("validates the Context7 key before marking it ready and keeps failed configuration editable", async () => {
  const state = coreFixture(); state.ready = false; state.items[4].configured = false;
  let fail = true;
  invokeMock.mockImplementation(async command => {
    if (command === "configure_context7") { if (fail) throw { message: "Chave inválida" }; return coreFixture(); }
    return state;
  });
  const user = userEvent.setup(); render(<CoreSettings />);
  await user.click(await screen.findByRole("button", { name: "Configurar Context7" }));
  const input = screen.getByLabelText("Chave de API");
  expect(input).toHaveAttribute("type", "password");
  expect(screen.getByRole("button", { name: "Salvar e verificar" })).toBeDisabled();
  await user.type(input, "test-only-key");
  await user.click(screen.getByRole("button", { name: "Salvar e verificar" }));
  expect(await screen.findByText("Chave inválida")).toBeVisible();
  expect(input).toHaveValue("test-only-key");
  fail = false;
  await user.click(screen.getByRole("button", { name: "Salvar e verificar" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(invokeMock).toHaveBeenCalledWith("configure_context7", { apiKey: "test-only-key" });
});

it("mantém a versão instalada quando uma atualização falha e permite nova tentativa", async () => {
  const state = coreFixture(); state.items[0].updateAvailable = true; state.items[0].latestVersion = "1.1.0";
  invokeMock.mockImplementation(async command => { if (command === "install_core_component") { state.items[0].error = "Download interrompido"; throw { message: "Download interrompido" }; } return state; });
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: "Atualizar Context-mode" }));
  expect(await screen.findByText("Download interrompido")).toBeInTheDocument();
  expect(screen.getAllByText("v1.0.0")).toHaveLength(6);
  await waitFor(() => expect(screen.getByRole("button", { name: "Atualizar Context-mode" })).toBeEnabled());
});

it("oferece reinstalação no card e no diagnóstico após uma atualização falhar", async () => {
  const state = coreFixture();
  state.items[3].latestVersion = "1.1.0";
  state.items[3].updateAvailable = true;
  state.items[3].error = "Recursos da nova release inválidos";
  invokeMock.mockResolvedValue(state);
  render(<CoreSettings />);

  const cardReinstall = await screen.findByRole("button", { name: "Reinstalar Open Design" });
  expect(screen.getByText("Atenção")).toBeInTheDocument();
  fireEvent.click(cardReinstall);
  expect(await screen.findByRole("alertdialog")).toHaveTextContent("baixada e verificada antes de substituir");
  fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));

  fireEvent.click(screen.getByRole("button", { name: "Diagnóstico e Reparo" }));
  const dialog = await screen.findByRole("dialog", { name: "Diagnóstico e Reparo" });
  expect(within(dialog).getByText("Core funcional · ação pendente")).toBeVisible();
  fireEvent.click(within(dialog).getByRole("button", { name: "Reinstalar Open Design" }));
  fireEvent.click(await screen.findByRole("button", { name: "Confirmar reinstalação" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("repair_core_component", { id: "open-design", reinstall: true }));
});
