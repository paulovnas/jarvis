import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { beforeEach, expect, it, vi } from "vitest";
import { ExecutionGrantsSettings } from "./ExecutionGrantsSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const call = vi.mocked(invoke);
const grant = {
  id: "grant-1",
  scope: "repository" as const,
  scopeRoot: "/projects/jarvis",
  matchKind: "commandPrefix" as const,
  duration: "persistent" as const,
  expiresAt: null,
  subject: "git push …",
  effects: { readsFilesystem: true, writesFilesystem: true, usesNetwork: true, controlsProcesses: false, destructive: false, dynamic: false, unknown: false },
  createdAt: 1_700_000_000_000,
  lastUsedAt: 1_700_000_001_000,
  uses: 3,
};

beforeEach(() => {
  call.mockReset().mockImplementation(async command => command === "list_execution_grants" ? [grant] : undefined);
  vi.mocked(toast.success).mockReset();
  vi.mocked(toast.error).mockReset();
});

it("lists sanitized project grants and revokes a selected rule after confirmation", async () => {
  const user = userEvent.setup();
  render(<ExecutionGrantsSettings projectId="p1" />);

  expect(screen.getByRole("status", { name: "Carregando autorizações de execução" })).toBeVisible();
  expect(await screen.findByText("git push …")).toBeVisible();
  expect(screen.getByText("Repositório")).toBeVisible();
  expect(screen.getByText("Persistente")).toBeVisible();
  expect(screen.getByText("Usos: 3", { exact: false })).toBeVisible();

  await user.click(screen.getByRole("button", { name: "Revogar autorização git push …" }));
  expect(screen.getByRole("alertdialog")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Revogar" }));

  await waitFor(() => expect(call).toHaveBeenCalledWith("revoke_execution_grant", { projectId: "p1", grantId: "grant-1" }));
  expect(await screen.findByText("Nenhuma autorização reutilizável")).toBeVisible();
  expect(toast.success).toHaveBeenCalledWith("Autorização de execução revogada");
});
