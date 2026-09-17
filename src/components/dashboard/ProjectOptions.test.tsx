import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { ProjectOptions } from "./ProjectOptions";
import { populatedLibrary } from "@/test/library-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const call = vi.mocked(invoke);
const project = populatedLibrary().projects[0];
const projectUpdater = {
  pending: false,
  error: null,
  clearError: vi.fn(),
  updateProject: vi.fn().mockResolvedValue(true),
};
const settings = {
  projectId: "p1",
  publishPrompt: "Review the diff and propose a cohesive commit.",
  prMode: "disabled" as const,
  prPrompt: "Use Summary and Validation sections.",
  ghAvailable: true,
};

beforeEach(() => {
  projectUpdater.clearError.mockReset();
  projectUpdater.updateProject.mockReset().mockResolvedValue(true);
  call.mockReset().mockImplementation(async command => {
    if (command === "get_project_publication_settings") return settings;
    if (command === "get_project_repositories") return [];
    if (command === "list_execution_grants") return [];
    if (command === "save_project_publication_settings") return { ...settings, prMode: "ask_pr" };
    throw new Error(`Unexpected command ${command}`);
  });
});

it("loads project-scoped publication rules and only reveals the PR editor when enabled", async () => {
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  expect(screen.getByRole("status", { name: "Carregando opções do projeto" })).toBeVisible();
  expect(await screen.findByRole("textbox", { name: "Instrução de publicação" })).toHaveValue(settings.publishPrompt);
  expect(call).toHaveBeenCalledWith("get_project_repositories", { projectId: "p1", includeDefault: false });
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
  call.mockImplementation(async command => {
    if (command === "get_project_publication_settings") return { ...settings, ghAvailable: false };
    if (command === "get_project_repositories" || command === "list_execution_grants") return [];
    throw new Error(`Unexpected command ${command}`);
  });
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  expect(await screen.findByText("GitHub CLI necessário")).toBeVisible();
  await user.click(screen.getByRole("combobox", { name: "Comportamento de pull request" }));
  expect(await screen.findByRole("option", { name: "Perguntar sobre PR" })).toHaveAttribute("aria-disabled", "true");
  expect(screen.getByRole("option", { name: "Perguntar sobre PR e merge" })).toHaveAttribute("aria-disabled", "true");
});

it("adds a named Git repository from a directory below the project root", async () => {
  vi.mocked(open).mockResolvedValue("/projects/jarvis/backend");
  const repository = { id: "r1", projectId: "p1", path: "backend", directory: "/projects/jarvis/backend", name: "backend", description: "API", branch: "main", upstream: "origin/main", ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, remoteUrl: "https://github.com/example/backend.git", available: true, error: null, createdAt: 1, updatedAt: 1 };
  call.mockImplementation(async (command) => {
    if (command === "get_project_publication_settings") return settings;
    if (command === "get_project_repositories") return [];
    if (command === "list_execution_grants") return [];
    if (command === "save_project_repository") return repository;
    throw new Error(`Unexpected command ${command}`);
  });
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  await screen.findByText("Nenhum repositório configurado");
  await user.click(screen.getByRole("button", { name: "Adicionar" }));
  expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true, defaultPath: "/projects/jarvis" }));
  expect(await screen.findByRole("dialog", { name: "Adicionar repositório" })).toBeVisible();
  await user.clear(screen.getByRole("textbox", { name: "Nome do repositório" }));
  await user.type(screen.getByRole("textbox", { name: "Nome do repositório" }), "Backend");
  await user.type(screen.getByRole("textbox", { name: "Descrição do repositório" }), "API");
  await user.click(screen.getByRole("button", { name: "Salvar repositório" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_repository", { projectId: "p1", repository: { directory: "/projects/jarvis/backend", name: "Backend", description: "API" } }));
  expect(toast.success).toHaveBeenCalledWith("Repositório adicionado");
});

it("updates the project name, directory, icon and color from Options", async () => {
  vi.mocked(open).mockResolvedValue("/projects/jarvis-next");
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  const name = await screen.findByRole("textbox", { name: "Nome do projeto" });
  await user.clear(name);
  await user.type(name, "Jarvis Next");
  await user.click(screen.getByRole("button", { name: "Alterar" }));
  await waitFor(() => expect(screen.getByRole("textbox", { name: "Pasta do projeto" })).toHaveValue("/projects/jarvis-next"));
  await user.click(screen.getByRole("button", { name: "Cor Roxo" }));
  await user.click(screen.getByRole("button", { name: "Ícone Foguete" }));
  await user.click(screen.getByRole("button", { name: "Salvar projeto" }));

  expect(open).toHaveBeenCalledWith({
    directory: true,
    multiple: false,
    defaultPath: "/projects/jarvis",
    title: "Selecionar pasta do projeto",
  });
  expect(projectUpdater.updateProject).toHaveBeenCalledWith("p1", {
    name: "Jarvis Next",
    path: "/projects/jarvis-next",
    icon: "rocket",
    color: "purple",
  });
});
