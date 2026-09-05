import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Home from "./Home";
import type { ProviderAccount } from "@/core/provider-accounts";
import { populatedLibrary } from "@/test/library-fixtures";
import { emptyChat } from "@/test/chat-fixtures";

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
    invokeMock.mockImplementation((command) => {
      if (command === "get_library_snapshot") return Promise.resolve(populatedLibrary());
      if (command === "get_chat") return Promise.resolve(emptyChat());
      if (command === "get_agent_activity") return Promise.resolve([]);
      if (command === "list_provider_accounts") return accountsMock();
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
  });

  it("renderiza a hierarquia persistida e o contexto da conversa selecionada", async () => {
    render(<Home />);

    expect(screen.getByRole("complementary", { name: "Workspace" })).toBeInTheDocument();
    await screen.findByRole("heading", { name: "Primeira conversa" });
    expect(screen.getByRole("combobox", { name: "Selecionar workspace" })).toHaveTextContent("Pessoal");
    expect(screen.getByRole("tab", { name: /projetos/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /conversas/i })).toBeInTheDocument();
    expect(screen.getByText("Projeto selecionado")).toBeInTheDocument();
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
      await screen.findByText("OpenAI Codex · openai-codex-pessoal"),
    ).toBeInTheDocument();
    expect(screen.getAllByText("GPT-5.6 Luna").length).toBeGreaterThan(0);
    expect(screen.getByText("GPT-5.6 Sol")).toBeInTheDocument();
    expect(screen.queryByText("Antigravity")).not.toBeInTheDocument();
    expect(screen.queryByText("Modelo desativado")).not.toBeInTheDocument();
    screen.getByRole("menuitem", { name: /GPT-5.6 Luna/ }).focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Extra alto" }));
    expect(modelButton).toHaveTextContent("GPT-5.6 Luna · Extra alto");
  });

  it("abre Configurações pelo callback da barra lateral", async () => {
    const user = userEvent.setup();
    render(<Home />);

    await user.click(screen.getByRole("button", { name: "Configurações" }));

    expect(
      await screen.findByRole("dialog", { name: "Configurações" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Provedores")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("list_provider_accounts");
  });

  it("does not restore stale models after Settings has supplied a newer account list", async () => {
    const user = userEvent.setup();
    let resolveInitial!: (accounts: ProviderAccount[]) => void;
    accountsMock.mockReturnValueOnce(new Promise<ProviderAccount[]>((resolve) => { resolveInitial = resolve; }));
    render(<Home />);
    const selector = await screen.findByRole("button", { name: "Selecionar modelo de IA" });
    await user.click(screen.getByRole("button", { name: "Configurações" }));
    expect(await screen.findByText("Nenhuma conta conectada")).toBeInTheDocument();

    await act(async () => resolveInitial([{
      alias: "openai-codex-removed", providerKind: "openai-codex", enabled: true, createdAt: 1,
      email: null, accountType: "personal", modelsAvailable: true,
      models: [{ id: "old", name: "Old model", reasoningLevels: [], defaultReasoningLevel: null }],
    }]));
    expect(selector).toHaveTextContent("Nenhum modelo conectado");
  });

});
