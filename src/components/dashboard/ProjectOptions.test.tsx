import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { markdownSource } from "@/test/markdown-editor";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { ProjectOptions } from "./ProjectOptions";
import { populatedLibrary } from "@/test/library-fixtures";
import { DEFAULT_HTTP_SETTINGS } from "@/core/http-client";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
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
const knowledge = { scopes: ["."], documents: [{ scope: ".", kind: "product", path: "prd.md", content: "# Product", essential: "", revision: "v1", sources: [], staleSources: [], error: null }] };

beforeEach(() => {
  projectUpdater.clearError.mockReset();
  projectUpdater.updateProject.mockReset().mockResolvedValue(true);
  call.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_project_publication_settings") return settings;
    if (command === "get_project_knowledge") return knowledge;
    if (command === "get_project_http_settings") return { ...DEFAULT_HTTP_SETTINGS, projectId: "p1", revision: 0 };
    if (command === "get_project_repositories") return [];
    if (command === "list_execution_grants") return [];
    if (command === "save_project_publication_settings") return { ...settings, ...(args as { settings: object }).settings };
    throw new Error(`Unexpected command ${command}`);
  });
});

it("loads HTTP options on demand and keeps its draft when switching sections", async () => {
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  expect(call).not.toHaveBeenCalledWith("get_project_http_settings", expect.anything());
  await user.click(screen.getByRole("tab", { name: "Cliente HTTP" }));
  await user.click(await screen.findByRole("button", { name: "Adicionar ambiente" }));
  await user.clear(screen.getByLabelText("Nome do ambiente 1"));
  await user.type(screen.getByLabelText("Nome do ambiente 1"), "Homologação");
  await user.click(screen.getByRole("tab", { name: "Geral" }));
  await user.click(screen.getByRole("tab", { name: "Cliente HTTP" }));
  expect(screen.getByLabelText("Nome do ambiente 1")).toHaveValue("Homologação");
  expect(call.mock.calls.filter(([command]) => command === "get_project_http_settings")).toHaveLength(1);
});

it("opens one section at a time and retains drafts without reloading visited sections", async () => {
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  const navigation = screen.getByRole("tablist", { name: "Opções do projeto" });
  expect(navigation).toHaveAttribute("aria-orientation", "vertical");
  expect(within(navigation).getAllByRole("tab").map(tab => tab.textContent)).toEqual(["Geral", "Conhecimento", "Aprendizados", "Repositórios", "Cliente HTTP", "Commit", "Autorizações"]);
  expect(call).not.toHaveBeenCalled();
  await user.clear(screen.getByRole("textbox", { name: "Nome do projeto" }));
  await user.type(screen.getByRole("textbox", { name: "Nome do projeto" }), "Rascunho do nome");
  await user.click(within(navigation).getByRole("tab", { name: "Conhecimento" }));
  const editor = await markdownSource(user, "Produto · Markdown");
  await user.type(editor, "\nProduct draft");
  expect(screen.queryByRole("textbox", { name: "Nome do projeto" })).not.toBeInTheDocument();
  await user.click(within(navigation).getByRole("tab", { name: "Geral" }));
  expect(screen.getByRole("textbox", { name: "Nome do projeto" })).toHaveValue("Rascunho do nome");
  await waitFor(() => expect(editor).not.toBeVisible());
  await user.keyboard("{ArrowDown}{Enter}");
  expect(await markdownSource(user, "Produto · Markdown")).toHaveValue("# Product\nProduct draft");
  expect(call.mock.calls.filter(([command]) => command === "get_project_knowledge")).toHaveLength(1);
});

it("keeps project knowledge usable when publication settings cannot load", async () => {
  call.mockImplementation(async command => {
    if (command === "get_project_knowledge") return knowledge;
    if (command === "get_project_repositories" || command === "list_execution_grants") return [];
    throw new Error("Publication unavailable");
  });
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  await user.click(screen.getByRole("tab", { name: "Commit" }));
  expect(await screen.findByText("Opções de publicação indisponíveis")).toBeVisible();
  await user.click(screen.getByRole("tab", { name: "Conhecimento" }));
  expect(await markdownSource(user, "Produto · Markdown")).toHaveValue("# Product");
});

it("edits commit instructions without showing or overwriting legacy PR options", async () => {
  call.mockImplementation(async (command, args) => {
    if (command === "get_project_publication_settings") return { ...settings, prMode: "ask_pr" };
    if (command === "save_project_publication_settings") return { ...settings, ...(args as { settings: object }).settings };
    throw new Error(`Unexpected command ${command}`);
  });
  const user = userEvent.setup();
  render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  await user.click(screen.getByRole("tab", { name: "Commit" }));
  expect(await markdownSource(user, "Instrução de publicação")).toHaveValue(settings.publishPrompt);
  expect(screen.queryByRole("textbox", { name: "Instrução e template da PR" })).not.toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "Comportamento de pull request" })).not.toBeInTheDocument();
  expect(screen.queryByText("Publicação assistida")).not.toBeInTheDocument();
  await user.clear(await markdownSource(user, "Instrução de publicação"));
  await user.type(await markdownSource(user, "Instrução de publicação"), "Create one focused commit.");
  await user.click(screen.getByRole("tab", { name: "Geral" }));
  await user.click(screen.getByRole("tab", { name: "Commit" }));
  expect(await markdownSource(user, "Instrução de publicação")).toHaveValue("Create one focused commit.");
  await user.click(screen.getByRole("button", { name: "Salvar opções" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_publication_settings", { projectId: "p1", settings: { publishPrompt: "Create one focused commit.", prMode: "ask_pr", prPrompt: settings.prPrompt } }));
  expect(toast.success).toHaveBeenCalledWith("Opções de publicação salvas");
});

it("does not reuse a knowledge draft for a different project", async () => {
  call.mockImplementation(async (command, args) => {
    if (command === "get_project_knowledge") return {
      ...knowledge,
      documents: knowledge.documents.map(document => ({ ...document, content: (args as { projectId: string }).projectId === "p2" ? "# Outro produto" : document.content })),
    };
    throw new Error(`Unexpected command ${command}`);
  });
  const user = userEvent.setup();
  const view = render(<ProjectOptions project={project} projectUpdater={projectUpdater} />);
  await user.type(screen.getByRole("textbox", { name: "Nome do projeto" }), " draft");
  await user.click(screen.getByRole("tab", { name: "Conhecimento" }));
  await user.type(await markdownSource(user, "Produto · Markdown"), "\nDraft from the previous project");
  view.rerender(<ProjectOptions project={{ ...project, id: "p2", name: "Outro projeto" }} projectUpdater={projectUpdater} />);
  expect(await markdownSource(user, "Produto · Markdown")).toHaveValue("# Outro produto");
  await user.click(screen.getByRole("tab", { name: "Geral" }));
  expect(screen.getByRole("textbox", { name: "Nome do projeto" })).toHaveValue("Outro projeto");
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
  await user.click(screen.getByRole("tab", { name: "Repositórios" }));
  await screen.findByText("Nenhum repositório configurado");
  expect(call).toHaveBeenCalledWith("get_project_repositories", { projectId: "p1", includeDefault: false });
  await user.click(screen.getByRole("button", { name: "Adicionar" }));
  expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true, defaultPath: "/projects/jarvis" }));
  expect(await screen.findByRole("dialog", { name: "Adicionar repositório" })).toBeVisible();
  await user.clear(screen.getByRole("textbox", { name: "Nome do repositório" }));
  await user.type(screen.getByRole("textbox", { name: "Nome do repositório" }), "Backend");
  await user.type(screen.getByRole("textbox", { name: "Descrição do repositório" }), "API");
  await user.type(screen.getByRole("textbox", { name: "Branch de referência" }), "hml");
  await user.click(screen.getByRole("button", { name: "Salvar repositório" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_repository", { projectId: "p1", repository: { directory: "/projects/jarvis/backend", name: "Backend", description: "API", referenceBranch: "hml" } }));
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
