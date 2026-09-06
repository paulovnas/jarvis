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
  vi.mocked(listen).mockImplementation(async (_name, callback) => { event = callback; return () => {}; });
});

it("bloqueia o chat com skeleton até confirmar o Core, inclusive em falhas", async () => {
  let reject!: (cause: unknown) => void;
  invokeMock.mockReturnValue(new Promise((_, fail) => { reject = fail; }));
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  expect(screen.getByRole("status", { name: "Carregando Jarvis" })).toBeInTheDocument();
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
  await act(async () => reject(new Error("offline")));
  expect(await screen.findByRole("button", { name: "Tentar novamente" })).toBeEnabled();
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
});

it("só libera o chat depois da instalação confirmada dos três componentes", async () => {
  let state = coreFixture(false);
  invokeMock.mockImplementation(async (name, args) => {
    if (name === "install_core_component") {
      const id = (args as { id: string }).id;
      state = { ...state, items: state.items.map(item => item.id === id ? { ...item, installed: true, installedVersion: "1.0.0" } : item) };
      state.ready = state.items.every(item => item.installed);
    }
    return state;
  });
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  fireEvent.click(await screen.findByRole("button", { name: "Instalar Context-mode" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Instalar Context-mode" })).not.toBeInTheDocument());
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Instalar Core" }));
  expect(await screen.findByText("Chat liberado")).toBeInTheDocument();
  expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "beads" });
});

it("bloqueia novamente se um componente obrigatório deixa de existir", async () => {
  invokeMock.mockResolvedValue(coreFixture());
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  expect(await screen.findByText("Chat liberado")).toBeInTheDocument();
  invokeMock.mockResolvedValue(coreFixture(false));
  await act(async () => event?.({ event: "core:changed", id: 1, payload: coreFixture(false) }));
  expect(await screen.findByRole("button", { name: "Instalar Core" })).toBeInTheDocument();
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
});

it("exige reparar regras inválidas do Ponytail mesmo quando há uma versão instalada", async () => {
  const healthy = coreFixture();
  const broken = {
    ...healthy,
    ready: false,
    items: healthy.items.map(item => item.id === "ponytail" ? {
      ...item,
      installed: false,
      installedVersion: "4.9.0",
      error: "As regras do Ponytail estão incompletas ou incompatíveis. Reinstale em Configurações → Geral → Core.",
    } : item),
  };
  invokeMock.mockImplementation(async name => name === "install_core_component" ? healthy : broken);
  render(<CoreGate><div>Chat liberado</div></CoreGate>);
  expect(await screen.findByText(/As regras do Ponytail estão incompletas/)).toBeInTheDocument();
  expect(screen.queryByText("Chat liberado")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Reinstalar Ponytail" }));
  expect(await screen.findByText("Chat liberado")).toBeInTheDocument();
  expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "ponytail" });
});
