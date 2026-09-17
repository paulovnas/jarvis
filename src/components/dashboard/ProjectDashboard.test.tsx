import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { bead, projectMetrics } from "@/test/dashboard-fixtures";
import { ProjectDashboard } from "./ProjectDashboard";
import { BeadsBoard } from "./BeadsBoard";
import { BeadDrawer } from "./BeadDrawer";
import { coreFixture } from "@/test/core-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const project = { id: "p1", workspaceId: "w1", name: "Jarvis", path: "/projects/jarvis", createdAt: 1 };
const projectUpdater = { pending: false, error: null, clearError: vi.fn(), updateProject: vi.fn().mockResolvedValue(true) };

describe("Project Dashboard", () => {
  beforeEach(() => {
    // jsdom has no layout; give responsive charts their actual container shape.
    const bounds = HTMLElement.prototype.getBoundingClientRect;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("recharts-responsive-container")
        ? new DOMRect(0, 0, 640, 240)
        : bounds.call(this);
    });
    call.mockReset();
    call.mockImplementation(async command => {
      if (command === "get_project_metrics") return projectMetrics();
      if (command === "get_project_beads") return [bead()];
      if (command === "get_core_status") return coreFixture();
      if (command === "get_project_publication_settings") return { projectId: "p1", publishPrompt: "Review changes", prMode: "disabled", prPrompt: "Write a PR", ghAvailable: true };
      if (command === "get_project_repositories") return [];
      if (command === "get_bead_detail") return { issue: bead(), comments: [] };
      if (command === "open_project_directory") return;
      throw new Error(`Unexpected command ${command}`);
    });
  });
  afterEach(() => vi.restoreAllMocks());
  it("loads real metrics and navigates from overview to sessions and board", async () => {
    const user = userEvent.setup(); const select = vi.fn();
    render(<ProjectDashboard project={project} projectUpdater={projectUpdater} onSelectSession={select} />);
    expect(screen.getByRole("status", { name: "Carregando detalhes" })).toBeInTheDocument();
    await screen.findByText("gpt-6-astra");
    expect(screen.getByText("1.500")).toBeInTheDocument();
    expect(screen.getByText("0/1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Conversa recente/ }));
    expect(select).toHaveBeenCalledWith("c1");
    await user.click(screen.getByRole("button", { name: /Ver quadro/ }));
    expect(await screen.findByRole("button", { name: "Tarefa: Validar integração" })).toBeInTheDocument();
  });
  it.each(["/projects/jarvis", "C:\\Users\\João Silva\\projetos\\Jarvis"])("opens the registered project instead of passing %s to the scoped frontend opener", async path => {
    const user = userEvent.setup();
    render(<ProjectDashboard project={{ ...project, path }} projectUpdater={projectUpdater} onSelectSession={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "Abrir pasta do projeto Jarvis" }));
    expect(call).toHaveBeenCalledWith("open_project_directory", { projectId: "p1" });
  });
  it("presents General, Kanban and project-scoped Options inside Details", async () => {
    const user = userEvent.setup();
    render(<ProjectDashboard project={project} projectUpdater={projectUpdater} onSelectSession={vi.fn()} />);
    expect(screen.getByRole("main", { name: "Detalhes de Jarvis" })).toBeVisible();
    expect(screen.getAllByRole("tab")).toHaveLength(3);
    await user.click(screen.getByRole("tab", { name: "Opções" }));
    expect(await screen.findByRole("textbox", { name: "Instrução de publicação" })).toHaveValue("Review changes");
    expect(call).toHaveBeenCalledWith("get_project_publication_settings", { projectId: "p1" });
  });
  it("reports an unavailable project directory without losing the dashboard", async () => {
    const report = vi.spyOn(toast, "error");
    const fallback = call.getMockImplementation();
    call.mockImplementation((command, args, options) => command === "open_project_directory"
      ? Promise.reject({ code: "project_directory", message: "A pasta do projeto não está disponível." })
      : fallback!(command, args, options));
    render(<ProjectDashboard project={project} projectUpdater={projectUpdater} onSelectSession={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Abrir pasta do projeto Jarvis" }));
    await waitFor(() => expect(report).toHaveBeenCalledWith("A pasta do projeto não está disponível."));
    expect(screen.getByRole("main", { name: "Detalhes de Jarvis" })).toBeVisible();
    report.mockRestore();
  });
  it("shows all statuses on demand, filters cards and opens readonly details", async () => {
    const user = userEvent.setup();
    render(<BeadsBoard projectId="p1" issues={[bead(), bead({ id: "second", title: "Planejar Core", issue_type: "epic", status: "deferred" })]} onChanged={vi.fn()} />);
    expect(screen.getByRole("region", { name: "Adiado: 1" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Estados vazios" }));
    expect(screen.getByRole("region", { name: "Vinculado: 0" })).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Buscar tarefas" }), "Vali");
    expect(screen.queryByRole("button", { name: "Épico: Planejar Core" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tarefa: Validar integração" }));
    const drawer = await screen.findByRole("dialog", { name: "Validar integração" });
    expect(await within(drawer).findByText("Contexto persistido")).toBeInTheDocument();
    expect(within(drawer).queryByRole("button", { name: /Editar|Excluir|Fechar tarefa/ })).not.toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("get_bead_detail", { projectId: "p1", issueId: bead().id });
  });
  it("filters only the closed lane by completion date and defaults to today", async () => {
    const user = userEvent.setup();
    const today = new Date();
    const fourDaysAgo = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 4, 12);
    render(<BeadsBoard projectId="p1" issues={[
      bead({ id: "open", title: "Ainda aberta" }),
      bead({ id: "closed-today", title: "Fechada hoje", status: "closed", closed_at: today.toISOString() }),
      bead({ id: "closed-before", title: "Fechada antes", status: "closed", closed_at: fourDaysAgo.toISOString() }),
    ]} onChanged={vi.fn()} />);
    expect(screen.getByRole("region", { name: "Fechado: 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tarefa: Fechada hoje" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Tarefa: Fechada antes" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tarefa: Ainda aberta" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Filtrar tarefas fechadas: Hoje" }));
    await user.click(screen.getByRole("button", { name: "7 Dias" }));
    expect(screen.getByRole("region", { name: "Fechado: 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tarefa: Fechada antes" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Filtrar tarefas fechadas: 7 Dias" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Filtrar tarefas fechadas: 7 Dias" }));
    await user.click(screen.getByRole("button", { name: "Personalizado" }));
    expect(screen.getAllByRole("grid")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(screen.getByRole("button", { name: "Filtrar tarefas fechadas: Personalizado" })).toBeVisible();
    expect(screen.getByRole("region", { name: "Fechado: 1" })).toBeInTheDocument();
  });
  it("preserves a failed comment and prevents duplicate submissions until persistence succeeds", async () => {
    const user = userEvent.setup(); const changed = vi.fn(async () => {});
    render(<BeadDrawer projectId="p1" issueId={bead().id} onClose={vi.fn()} onSelect={vi.fn()} onChanged={changed} />);
    const input = await screen.findByRole("textbox", { name: "Adicionar comentário" });
    expect(input).toHaveAttribute("spellcheck", "true");
    expect(input).toHaveAttribute("autocorrect", "on");
    expect(input).toHaveAttribute("autocapitalize", "sentences");
    await user.type(input, "Validado no projeto");
    call.mockRejectedValueOnce({ message: "Banco ocupado" });
    await user.click(screen.getByRole("button", { name: "Comentar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Banco ocupado");
    expect(input).toHaveValue("Validado no projeto");
    let resolve!: (value: unknown) => void;
    call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    await user.dblClick(screen.getByRole("button", { name: "Comentar" }));
    expect(call.mock.calls.filter(([command]) => command === "add_bead_comment")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Fechar detalhes" })).toBeDisabled();
    await act(async () => resolve({ id: "1", text: "Validado no projeto", author: "Você", created_at: "2026-09-05T15:00:00Z" }));
    await waitFor(() => expect(input).toHaveValue(""));
    expect(screen.getByText("Validado no projeto")).toBeInTheDocument();
    expect(changed).toHaveBeenCalled();
  });
  it("discards an old project response after switching project", async () => {
    let resolve!: (value: unknown) => void;
    call.mockImplementation(command => command === "get_project_metrics" ? new Promise(done => { resolve = done; }) : Promise.resolve([]));
    const view = render(<ProjectDashboard key="p1" project={project} projectUpdater={projectUpdater} onSelectSession={vi.fn()} />);
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_project_metrics", { projectId: "p1" }));
    const old = resolve;
    view.rerender(<ProjectDashboard key="p2" project={{ ...project, id: "p2", name: "Outro" }} projectUpdater={projectUpdater} onSelectSession={vi.fn()} />);
    await act(async () => old(projectMetrics()));
    expect(screen.queryByText("gpt-6-astra")).not.toBeInTheDocument();
    expect(screen.getByRole("main", { name: "Detalhes de Outro" })).toBeInTheDocument();
  });
});
