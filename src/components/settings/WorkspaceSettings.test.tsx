import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { WorkspaceSettings } from "./WorkspaceSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const storage = [{ workspace: { id: "w1", name: "Pessoal" }, conversations: 3, bytes: 2048, projects: [{ project: { id: "p1", name: "Jarvis", path: "/code/jarvis" }, conversations: 3, bytes: 2048 }] }];
beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(invoke).mockResolvedValue(storage); });

it("shows per-project chat storage and confirms workspace deletion without implying source deletion", async () => {
  const user = userEvent.setup();
  render(<WorkspaceSettings />);
  expect(screen.getByRole("status", { name: "Medindo históricos dos workspaces" })).toBeVisible();
  expect(await screen.findByText("Pessoal")).toBeVisible();
  expect(screen.getByText("2 KB em históricos")).toBeVisible();
  expect(screen.getByText("2 KB")).toBeVisible();
  expect(screen.getByText(/As pastas dos projetos não entram no cálculo/)).toBeVisible();
  expect(screen.queryByText("/code/jarvis")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Históricos dos projetos de Pessoal")).toHaveTextContent("Jarvis3 conversas2 KB");
  await user.click(screen.getByRole("button", { name: "Excluir workspace Pessoal" }));
  const dialog = screen.getByRole("alertdialog");
  expect(dialog).toHaveTextContent("As pastas dos projetos e seus arquivos permanecerão intactos");
  expect(dialog).toHaveTextContent("todas as conversas (3)");
  expect(invoke).not.toHaveBeenCalledWith("delete_library_item", expect.anything());
  await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
  await user.click(screen.getByRole("button", { name: "Excluir workspace Pessoal" }));
  vi.mocked(invoke).mockImplementation(async command => command === "get_workspace_storage" ? [] : {});
  await user.click(screen.getByRole("button", { name: "Excluir workspace" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_library_item", { target: { kind: "workspace", id: "w1" }, confirmed: true }));
  expect(await screen.findByText("Nenhum workspace cadastrado.")).toBeVisible();
});

it("keeps deletion open when running conversations prevent removal", async () => {
  const user = userEvent.setup(); render(<WorkspaceSettings />);
  await user.click(await screen.findByRole("button", { name: "Excluir workspace Pessoal" }));
  vi.mocked(invoke).mockRejectedValue({ message: "Interrompa as conversas em execução antes de excluir este item." });
  await user.click(screen.getByRole("button", { name: "Excluir workspace" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Interrompa as conversas");
  expect(screen.getByRole("alertdialog")).toBeVisible();
});
