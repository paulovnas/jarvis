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

it("analisa ao solucionar e só libera o chat com os seis componentes prontos", async () => {
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
  invokeMock.mockImplementation(async name => {
    if (name === "repair_core_component") throw { message: "Sem conexão" };
    return coreFixture(false);
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
