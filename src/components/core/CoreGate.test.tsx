import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { coreFixture } from "@/test/core-fixtures";
import { CoreGate } from "./CoreGate";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const invokeMock = vi.mocked(invoke);
let event: EventCallback<unknown> | undefined;
beforeEach(() => {
  invokeMock.mockReset(); event = undefined;
  vi.mocked(listen).mockImplementation(async (name, callback) => { if (name === "core:changed") event = callback; return () => {}; });
});

it("bloqueia o chat com skeleton até confirmar o Core, inclusive em falhas", async () => {
  let reject!: (cause: unknown) => void;
  invokeMock.mockReturnValue(new Promise((_, fail) => { reject = fail; }));
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  expect(screen.getByRole("status", { name: "Carregando Jarvis" })).toBeInTheDocument();
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
  await act(async () => reject(new Error("offline")));
  expect(await screen.findByRole("button", { name: "Solucionar" })).toBeEnabled();
  expect(screen.getByRole("alert")).toHaveTextContent("Uso do Jarvis pausado");
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
});

it("analisa ao solucionar e só libera o chat com os componentes essenciais prontos", async () => {
  invokeMock.mockResolvedValue(coreFixture(false));
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  fireEvent.click(await screen.findByRole("button", { name: "Solucionar" }));
  expect(await screen.findByRole("dialog", { name: "Diagnóstico e Reparo" })).toBeVisible();
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("diagnose_core"));
  await waitFor(() => expect(screen.getByRole("button", { name: "Fechar" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Fechar" }));
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
  invokeMock.mockResolvedValue(coreFixture());
  await act(async () => event?.({ event: "core:changed", id: 1, payload: coreFixture() }));
  expect(await screen.findByText("Chat liberado")).toBeInTheDocument();
});

it("bloqueia novamente se um componente obrigatório deixa de existir", async () => {
  invokeMock.mockResolvedValue(coreFixture());
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  expect(await screen.findByText("Chat liberado")).toBeInTheDocument();
  await act(async () => event?.({ event: "core:changed", id: 1, payload: coreFixture(false) }));
  expect(await screen.findByRole("button", { name: "Solucionar" })).toBeInTheDocument();
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
});

it("repara uma falha de execução mesmo quando os arquivos estão instalados", async () => {
  const healthy = coreFixture();
  const broken = { ...healthy, ready: false, items: healthy.items.map(item => item.id === "ponytail" ? {
    ...item, healthError: "Regras inválidas", diagnostics: [{ label: "Arquivos e recursos", passed: false, message: "Regras inválidas" }],
  } : item) };
  invokeMock.mockImplementation(async name => name === "repair_core_component" ? healthy : broken);
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  fireEvent.click(await screen.findByRole("button", { name: "Solucionar" }));
  expect(await screen.findByText("Regras inválidas")).toBeInTheDocument();
  const repair = screen.getByRole("button", { name: "Reparar Ponytail" });
  await waitFor(() => expect(repair).toBeEnabled());
  fireEvent.click(repair);
  const back = await screen.findByRole("button", { name: "Voltar ao Jarvis" });
  fireEvent.click(back);
  expect(await screen.findByText("Chat liberado")).toBeInTheDocument();
  expect(invokeMock).toHaveBeenCalledWith("repair_core_component", { id: "ponytail", reinstall: false });
});

it("exige confirmação para reinstalar e mantém o bloqueio se o reparo falhar", async () => {
  const state = coreFixture(false);
  state.items[0].installedVersion = "1.0.0";
  invokeMock.mockImplementation(async name => {
    if (name === "repair_core_component") throw { message: "Sem conexão" };
    return state;
  });
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  fireEvent.click(await screen.findByRole("button", { name: "Solucionar" }));
  const reinstall = await screen.findByRole("button", { name: "Reinstalar Context-mode" });
  await waitFor(() => expect(reinstall).toBeEnabled());
  fireEvent.click(reinstall);
  expect(await screen.findByRole("alertdialog")).toHaveTextContent("Projetos, conversas e chaves serão preservados");
  fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(invokeMock).not.toHaveBeenCalledWith("repair_core_component", expect.anything());
  fireEvent.click(reinstall);
  fireEvent.click(await screen.findByRole("button", { name: "Confirmar reinstalação" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("repair_core_component", { id: "context-mode", reinstall: true }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Reinstalar Context-mode" })).toBeEnabled());
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
});

it("não confunde uma consulta de atualização sem rede com Core quebrado", async () => {
  const state = coreFixture();
  state.items[0].error = "Não foi possível consultar o GitHub";
  invokeMock.mockResolvedValue(state);
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  expect(await screen.findByText("Chat liberado")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Solucionar" })).not.toBeInTheDocument();
});

it.each(["openmontage", "comfyui", "graft"] as const)("requires the %s runtime before starting a chat", async id => {
  const state = coreFixture();
  const video = state.items.find(item => item.id === id)!;
  video.installed = false; video.configured = false; video.installedVersion = null;
  state.ready = false;
  invokeMock.mockResolvedValue(state);
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  expect(await screen.findByRole("button", { name: "Solucionar" })).toBeEnabled();
  expect(screen.getByText(video.name)).toBeVisible();
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
});

it("installs required Graft from upgrade diagnostics before releasing the chat", async () => {
  const state = coreFixture();
  const graft = state.items.find(item => item.id === "graft")!;
  graft.installed = false; graft.configured = false; graft.installedVersion = null;
  state.ready = false;
  invokeMock.mockImplementation(async name => name === "install_core_component" ? coreFixture() : state);
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  fireEvent.click(await screen.findByRole("button", { name: "Solucionar" }));
  const install = await screen.findByRole("button", { name: "Instalar Graft" });
  await waitFor(() => expect(install).toBeEnabled());
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
  fireEvent.click(install);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "graft" }));
  fireEvent.click(await screen.findByRole("button", { name: "Voltar ao Jarvis" }));
  expect(await screen.findByText("Chat liberado")).toBeVisible();
});

it("installs missing openmontage resources from upgrade diagnostics and releases the chat", async () => {
  const state = coreFixture();
  const openmontage = state.items.find(item => item.id === "openmontage")!;
  openmontage.installed = false;
  openmontage.configured = false;
  openmontage.installedVersion = null;
  state.ready = false;
  invokeMock.mockImplementation(async name => name === "install_core_component" ? coreFixture() : state);
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  fireEvent.click(await screen.findByRole("button", { name: "Solucionar" }));
  const install = await screen.findByRole("button", { name: "Instalar OpenMontage" });
  await waitFor(() => expect(install).toBeEnabled());
  expect(screen.getByText(/conforme as dependências e credenciais configuradas/)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Reparar OpenMontage" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Reinstalar OpenMontage" })).not.toBeInTheDocument();
  fireEvent.click(install);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "openmontage" }));
  fireEvent.click(await screen.findByRole("button", { name: "Voltar ao Jarvis" }));
  expect(await screen.findByText("Chat liberado")).toBeVisible();
});

it("allows an openmontage download to be canceled from upgrade diagnostics", async () => {
  let state = coreFixture();
  state.ready = false;
  const audio = state.items.find(item => item.id === "openmontage")!;
  audio.installed = false; audio.configured = false; audio.installedVersion = null;
  audio.stage = "Baixando modelos de voz e música";
  invokeMock.mockImplementation(async command => {
    if (command === "cancel_core_installation") {
      state = structuredClone(state);
      state.items.find(item => item.id === "openmontage")!.stage = null;
    }
    return state;
  });
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  fireEvent.click(await screen.findByRole("button", { name: "Solucionar" }));
  fireEvent.click(await screen.findByRole("button", { name: "Cancelar instalação de OpenMontage" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cancel_core_installation", { id: "openmontage" }));
  await waitFor(() => expect(screen.queryByRole("progressbar", { name: "Instalação de OpenMontage" })).not.toBeInTheDocument());
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
});
