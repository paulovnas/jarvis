import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { coreFixture } from "@/test/core-fixtures";
import { CoreSettings } from "./CoreSettings";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const invokeMock = vi.mocked(invoke);
beforeEach(() => invokeMock.mockReset());

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

it("mantém a versão instalada quando uma atualização falha e permite nova tentativa", async () => {
  const state = coreFixture(); state.items[0].updateAvailable = true; state.items[0].latestVersion = "1.1.0";
  invokeMock.mockImplementation(async command => { if (command === "install_core_component") { state.items[0].error = "Download interrompido"; throw { message: "Download interrompido" }; } return state; });
  render(<CoreSettings />);
  fireEvent.click(await screen.findByRole("button", { name: "Atualizar Context-mode" }));
  expect(await screen.findByText("Download interrompido")).toBeInTheDocument();
  expect(screen.getAllByText("v1.0.0")).toHaveLength(3);
  await waitFor(() => expect(screen.getByRole("button", { name: "Atualizar Context-mode" })).toBeEnabled());
});
