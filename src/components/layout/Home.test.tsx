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

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);
const accountsMock = vi.fn<() => Promise<ProviderAccount[]>>();

describe("Home shell", () => {
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
  });

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
    expect(screen.getByText("Subagentes")).toBeInTheDocument();
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
