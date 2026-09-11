import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { ProjectOptions } from "./ProjectOptions";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const call = vi.mocked(invoke);
const settings = {
  projectId: "p1",
  publishPrompt: "Review the diff and propose a cohesive commit.",
  prMode: "disabled" as const,
  prPrompt: "Use Summary and Validation sections.",
  ghAvailable: true,
};

beforeEach(() => {
  call.mockReset().mockImplementation(async command => {
    if (command === "get_project_publication_settings") return settings;
    if (command === "save_project_publication_settings") return { ...settings, prMode: "ask_pr" };
    throw new Error(`Unexpected command ${command}`);
  });
});

it("loads project-scoped publication rules and only reveals the PR editor when enabled", async () => {
  const user = userEvent.setup();
  render(<ProjectOptions projectId="p1" />);
  expect(screen.getByRole("status", { name: "Carregando opções do projeto" })).toBeVisible();
  expect(await screen.findByRole("textbox", { name: "Instrução de publicação" })).toHaveValue(settings.publishPrompt);
  expect(screen.queryByRole("textbox", { name: "Instrução e template da PR" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("combobox", { name: "Comportamento de pull request" }));
  await user.click(await screen.findByRole("option", { name: "Perguntar sobre PR" }));
  expect(screen.getByRole("textbox", { name: "Instrução e template da PR" })).toHaveValue(settings.prPrompt);
  await user.clear(screen.getByRole("textbox", { name: "Instrução de publicação" }));
  await user.type(screen.getByRole("textbox", { name: "Instrução de publicação" }), "Create one focused commit.");
  await user.click(screen.getByRole("button", { name: "Salvar opções" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_publication_settings", { projectId: "p1", settings: { publishPrompt: "Create one focused commit.", prMode: "ask_pr", prPrompt: settings.prPrompt } }));
  expect(toast.success).toHaveBeenCalledWith("Opções de publicação salvas");
});

it("keeps PR automation unavailable when GitHub CLI is missing", async () => {
  call.mockResolvedValueOnce({ ...settings, ghAvailable: false });
  const user = userEvent.setup();
  render(<ProjectOptions projectId="p1" />);
  expect(await screen.findByText("GitHub CLI necessário")).toBeVisible();
  await user.click(screen.getByRole("combobox", { name: "Comportamento de pull request" }));
  expect(await screen.findByRole("option", { name: "Perguntar sobre PR" })).toHaveAttribute("aria-disabled", "true");
  expect(screen.getByRole("option", { name: "Perguntar sobre PR e merge" })).toHaveAttribute("aria-disabled", "true");
});
