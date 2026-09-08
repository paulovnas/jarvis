import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Home from "./Home";
import type { ProviderAccount } from "@/core/provider-accounts";
import { populatedLibrary } from "@/test/library-fixtures";
import { emptyChat } from "@/test/chat-fixtures";
import { bead, projectMetrics } from "@/test/dashboard-fixtures";
import { coreFixture } from "@/test/core-fixtures";
import { DEFAULT_DESKTOP_LAYOUT } from "@/core/desktop-layout";
import { DesktopLayoutProvider } from "./DesktopLayoutProvider";
import type { FilePreview } from "@/core/project-files";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("@/components/files/CodeViewer", () => ({ default: ({ file }: { file: FilePreview }) => <pre aria-label={`Arquivo ${file.path}`}>{file.content}</pre> }));
vi.mock("@/components/chat/TerminalSurface", () => ({ TerminalSurface: () => <div role="application" aria-label="Terminal em execução" /> }));

const invokeMock = vi.mocked(invoke);
const accountsMock = vi.fn<() => Promise<ProviderAccount[]>>();

describe("Home shell", () => {
  it("opens Explorer files beside Chat while preserving the composer and bottom terminal, then restores the Explorer section", async () => {
    const user = userEvent.setup();
    accountsMock.mockResolvedValue([{ alias: "preview", providerKind: "openai-codex", enabled: true, createdAt: 1, email: null, accountType: "personal", modelsAvailable: true, showUsage: false, models: [{ id: "model", name: "Modelo", reasoningLevels: [], defaultReasoningLevel: null }] }]);
    const original = invokeMock.getMockImplementation()!;
    let layout = { ...DEFAULT_DESKTOP_LAYOUT, terminalPanels: { c1: { open: true, size: 40, activeTerminalId: "term-1" } } };
    invokeMock.mockImplementation(async (command, args, options) => {
      if (command === "get_desktop_layout") return layout;
      if (command === "save_desktop_layout") { layout = (args as { layout: typeof layout }).layout; return; }
      if (command === "list_chat_terminals") return [{ id: "term-1", conversationId: "c1", title: "Terminal 1", cwd: "/project", pid: 1, startedAt: 1, endedAt: null, exitCode: null, status: "running", origin: "user" }];
      if (command === "list_chat_processes") return [];
      if (command === "list_project_directory") return { path: "", entries: [{ name: "README.md", path: "README.md", kind: "file" }], truncated: false };
      if (command === "read_project_file") return { path: "README.md", content: "# Meu projeto", size: 13, encoding: "UTF-8" };
      return original(command, args, options);
    });
    const first = render(<DesktopLayoutProvider><Home /></DesktopLayoutProvider>);
    const composer = await screen.findByRole("textbox", { name: "Mensagem" });
    expect(composer).toHaveAttribute("contenteditable", "true");
    expect(composer).toBeVisible();
    // jsdom has no panel geometry; focus directly instead of hitting a resize
    // handle at the synthetic pointer's (0, 0) position.
    composer.focus();
    expect(composer).toHaveFocus();
    await user.keyboard("Rascunho preservado");
    expect(composer).toHaveTextContent("Rascunho preservado");
    const terminal = await screen.findByRole("application", { name: "Terminal em execução" });
    expect(invokeMock).not.toHaveBeenCalledWith("list_project_directory", expect.anything());
    await user.click(screen.getByRole("tab", { name: "Explorer" }));
    await user.click(await screen.findByRole("treeitem", { name: "README.md" }));
    expect(await screen.findByLabelText("Arquivo README.md")).toHaveTextContent("# Meu projeto");
    expect(terminal).toBeVisible();
    expect(screen.getByRole("application", { name: "Terminal em execução" })).toBe(terminal);
    expect(composer).not.toBeVisible();
    await user.click(screen.getByRole("tab", { name: "Chat" }));
    expect(screen.getByRole("textbox", { name: "Mensagem" })).toBe(composer);
    expect(composer).toHaveTextContent("Rascunho preservado");
    await waitFor(() => expect(layout.inspectorTab).toBe("explorer"));
    first.unmount();
    render(<DesktopLayoutProvider><Home /></DesktopLayoutProvider>);
    expect(await screen.findByRole("treeitem", { name: "README.md" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "Explorer" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "README.md" })).toBeVisible();
  }, 15_000);
  it("offers only adding a project while the selected workspace is empty and unlocks after success", async () => {
    const user = userEvent.setup();
    const snapshot = populatedLibrary(); snapshot.projects = []; snapshot.conversations = []; snapshot.selection.projectId = null; snapshot.selection.conversationId = null;
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_library_snapshot") return snapshot;
      if (command === "add_project") return populatedLibrary();
      if (command === "get_chat") return emptyChat();
      return [];
    });
    render(<Home />);
    const add = await screen.findByRole("button", { name: "Adicionar projeto" });
    expect(screen.getAllByRole("button")).toEqual([add]);
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    await user.click(add);
    expect(invokeMock).toHaveBeenCalledWith("add_project", { workspaceId: "w1" });
    expect(await screen.findByRole("button", { name: "Configurações" })).toBeEnabled();
    expect(screen.getByRole("complementary", { name: "Workspace" })).toBeVisible();
  });
  beforeEach(() => {
    vi.restoreAllMocks();
    invokeMock.mockReset();
    accountsMock.mockReset().mockResolvedValue([]);
    invokeMock.mockImplementation((command, args) => {
      if (command === "get_agent_models") return Promise.resolve({});
      if (command === "get_workflow") return Promise.resolve(null);
      if (command === "set_agent_model") {
        const selection = args as { flow: string; role: string; choice: { account: string; model: string; reasoning: string | null } };
        return Promise.resolve({ [`${selection.flow}/${selection.role}`]: selection.choice });
      }
      if (command === "get_library_snapshot") return Promise.resolve(populatedLibrary());
      if (command === "get_chat") return Promise.resolve(emptyChat());
      if (command === "get_agent_activity") return Promise.resolve([]);
      if (command === "get_project_beads" || command === "get_agent_file_changes") return Promise.resolve([]);
      if (command === "list_provider_accounts") return accountsMock();
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
  });

  it("abre o Kanban do projeto pelo resumo do plano", async () => {
    const user = userEvent.setup();
    const snapshot = populatedLibrary();
    const projectId = snapshot.selection.projectId;
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "get_library_snapshot") return snapshot;
      if (command === "get_chat") return emptyChat();
      if (command === "get_project_beads") return [bead({ issue_type: "epic", title: "Plano de integração" })];
      if (command === "get_project_metrics") return { ...projectMetrics(), projectId };
      if (command === "get_core_status") return coreFixture();
      if (command === "select_library_item") {
        expect(args).toEqual({ target: { kind: "project", id: projectId } });
        return { ...snapshot, selection: { ...snapshot.selection, conversationId: null } };
      }
      return [];
    });
    render(<Home />);
    await user.click(await screen.findByRole("button", { name: "Plano: Plano de integração" }));
    await user.click(await screen.findByRole("button", { name: "Ver mais detalhes" }));
    expect(await screen.findByRole("tab", { name: /Kanban/ })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByRole("button", { name: "Épico: Plano de integração" })).toBeInTheDocument();
  }, 10_000);

  it("renderiza a hierarquia persistida e o contexto da conversa selecionada", async () => {
    render(<Home />);

    expect(screen.getByRole("complementary", { name: "Workspace" })).toBeInTheDocument();
    await screen.findByRole("heading", { name: "Primeira conversa" });
    expect(screen.getByRole("combobox", { name: "Selecionar workspace" })).toHaveTextContent("Pessoal");
    expect(screen.getByRole("button", { name: "Novo" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Detalhes" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Atividades" })).not.toBeInTheDocument();
    expect(screen.getByRole("main", { name: "Conversa" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Primeira conversa" })).toBeInTheDocument();
    expect(screen.getByText("Arquivos alterados")).toBeInTheDocument();
    expect(screen.getByText("Plano")).toBeInTheDocument();
    expect(screen.queryByText("Subagentes")).not.toBeInTheDocument();
    expect(screen.getByText("Contexto")).toBeInTheDocument();
  });

  it("lista no seletor apenas os modelos das contas conectadas", async () => {
    const user = userEvent.setup();
    accountsMock.mockResolvedValueOnce([
      {
        alias: "openai-codex-pessoal",
        providerKind: "openai-codex",
        enabled: true,
        createdAt: 1_735_689_600,
        email: "dev@example.com",
        accountType: "personal",
        modelsAvailable: true,
        models: [
          { id: "gpt-5.6-luna", name: "GPT-5.6 Luna", reasoningLevels: ["medium", "xhigh"], defaultReasoningLevel: "medium" },
          { id: "gpt-5.6-sol", name: "GPT-5.6 Sol", reasoningLevels: [], defaultReasoningLevel: null },
        ],
      },
      { alias: "openai-codex-disabled", providerKind: "openai-codex", enabled: false, createdAt: 1, email: null, accountType: "personal", modelsAvailable: true,
        models: [{ id: "disabled-model", name: "Modelo desativado", reasoningLevels: [], defaultReasoningLevel: null }] },
    ]);
    render(<Home />);

    const modelButton = await screen.findByRole("button", { name: "Selecionar modelo de IA" });
    await waitFor(() => expect(modelButton).toHaveTextContent("GPT-5.6 Luna · Médio"));
    screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus();
    await user.keyboard("{Enter}");

    expect(
      await screen.findByText("openai-codex-pessoal"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /GPT-5.6 Luna/ })).not.toBeInTheDocument();
    screen.getByRole("menuitem", { name: "openai-codex-pessoal" }).focus();
    await user.keyboard("{ArrowRight}");
    await screen.findByRole("menuitem", { name: /GPT-5.6 Luna/ });
    expect(screen.getAllByText("GPT-5.6 Luna").length).toBeGreaterThan(0);
    expect(screen.getByText("GPT-5.6 Sol")).toBeInTheDocument();
    expect(screen.queryByText("Antigravity")).not.toBeInTheDocument();
    expect(screen.queryByText("Modelo desativado")).not.toBeInTheDocument();
    screen.getByRole("menuitem", { name: /GPT-5.6 Luna/ }).focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Extra alto" }));
    await waitFor(() => expect(modelButton).toHaveTextContent("GPT-5.6 Luna · Extra alto"));
  });

  it("abre Configurações pela statusbar em uma modal central", async () => {
    const user = userEvent.setup();
    render(<Home />);

    await user.click(screen.getByRole("button", { name: "Configurações" }));

    expect(
      await screen.findByRole("dialog", { name: "Configurações" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Configurações" })).not.toHaveAttribute("data-side");
    expect(screen.getByText("Provedores")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("list_provider_accounts");
  });

  it("recolhe e reabre ambas as barras pelo header sem perder o rascunho", async () => {
    const user = userEvent.setup();
    render(<Home />);
    const field = await screen.findByRole("textbox", { name: "Mensagem" });
    field.focus();
    expect(field).toHaveFocus();
    await user.paste("Rascunho preservado");
    expect(field).toHaveTextContent("Rascunho preservado");
    await user.click(screen.getByRole("button", { name: "Recolher barra lateral" }));
    await user.click(screen.getByRole("button", { name: "Recolher inspector" }));
    expect(screen.queryByRole("complementary", { name: "Workspace" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Atividades" })).not.toBeInTheDocument();
    expect(screen.queryByRole("separator", { name: "Redimensionar barra lateral" })).not.toBeInTheDocument();
    expect(screen.queryByRole("separator", { name: "Redimensionar inspector" })).not.toBeInTheDocument();
    expect(field).toHaveTextContent("Rascunho preservado");
    await user.click(screen.getByRole("button", { name: "Abrir barra lateral" }));
    await user.click(screen.getByRole("button", { name: "Abrir inspector" }));
    expect(screen.getByRole("complementary", { name: "Workspace" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeInTheDocument();
    expect(screen.getByRole("separator", { name: "Redimensionar barra lateral" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("separator", { name: "Redimensionar inspector" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("textbox")).toBe(field);
  });

  it("does not restore stale models after Settings has supplied a newer account list", async () => {
    const user = userEvent.setup();
    let resolveInitial!: (accounts: ProviderAccount[]) => void;
    accountsMock.mockReturnValueOnce(new Promise<ProviderAccount[]>((resolve) => { resolveInitial = resolve; }));
    render(<Home />);
    const selector = await screen.findByRole("button", { name: "Selecionar modelo de IA" });
    await user.click(screen.getByRole("button", { name: "Configurações" }));
    await user.click(await screen.findByRole("tab", { name: /Provedores/ }));
    expect(await screen.findByText("Nenhuma conta conectada")).toBeInTheDocument();

    await act(async () => resolveInitial([{
      alias: "openai-codex-removed", providerKind: "openai-codex", enabled: true, createdAt: 1,
      email: null, accountType: "personal", modelsAvailable: true,
      models: [{ id: "old", name: "Old model", reasoningLevels: [], defaultReasoningLevel: null }],
    }]));
    expect(selector).toHaveTextContent("Nenhum modelo conectado");
  });

});
