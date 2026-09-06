import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { ProviderAccount } from "@/core/provider-accounts";
import { WebSearchSettings } from "./WebSearchSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const invokeMock = vi.mocked(invoke);
const account = (alias: string, providerKind = "openai-codex"): ProviderAccount => ({
  alias, providerKind, enabled: true, models: [{ id: "gpt-5.6-luna", name: "Luna", reasoningLevels: [], defaultReasoningLevel: null }], modelsAvailable: true, createdAt: 1, email: null, accountType: "personal",
});

describe("WebSearchSettings", () => {
  it.each(["web_search", "vision"] as const)("herda chat e permite restaurar herança em %s", async kind => {
    const title = kind === "vision" ? "Vision" : "Web Search";
    invokeMock.mockImplementation(async (command, args) => command.startsWith("get_") ? { accountAlias: null, model: null, inheritChat: true } : args);
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[account("openai-codex-paulo")]} kind={kind} />);
    const select = await screen.findByRole("combobox", { name: `Provedor de ${title}` });
    expect(select).toHaveTextContent("Herdar do chat");
    expect(screen.getByRole("combobox", { name: `Modelo de ${title}` })).toBeDisabled();
    await user.click(select);
    await user.click(await screen.findByRole("option", { name: "Desligado" }));
    await waitFor(() => expect(select).toHaveTextContent("Desligado"));
    await waitFor(() => expect(select).toBeEnabled());
    await user.click(select);
    await user.click(await screen.findByRole("option", { name: "Herdar do chat" }));
    await waitFor(() => expect(select).toHaveTextContent("Herdar do chat"));
    expect(invokeMock).toHaveBeenLastCalledWith(`set_${kind}_config`, { accountAlias: null, model: null, inheritChat: true });
  });
  it("salva o modelo escolhido para Vision sem alterar o Web Search", async () => {
    const google = account("antigravity-pessoal", "antigravity");
    google.models = ["gemini-3.8-flash", "gemini-pro"].map(id => ({ id, name: id, reasoningLevels: [], defaultReasoningLevel: null }));
    invokeMock.mockImplementation(async (command, args) => command === "get_vision_config" ? { accountAlias: google.alias, model: "gemini-pro" } : args);
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[google]} kind="vision" />);
    await user.click(await screen.findByRole("combobox", { name: "Modelo de Vision" }));
    await user.click(await screen.findByRole("option", { name: "gemini-3.8-flash" }));
    expect(invokeMock).toHaveBeenCalledWith("set_vision_config", { inheritChat: false, accountAlias: google.alias, model: "gemini-3.8-flash" });
    expect(invokeMock).not.toHaveBeenCalledWith("set_web_search_config", expect.anything());
    expect(screen.getByRole("combobox", { name: "Modelo de Vision" })).toHaveTextContent("gemini-3.8-flash");
  });
  it("permite trocar o modelo de Web Search mantendo sua conta", async () => {
    const search = account("openai-codex-pesquisa");
    search.models.push({ id: "gpt-5.6-sol", name: "Sol", reasoningLevels: [], defaultReasoningLevel: null });
    invokeMock.mockImplementation(async (command, args) => command === "get_web_search_config" ? { accountAlias: search.alias, model: "gpt-5.6-luna" } : args);
    const user = userEvent.setup(); render(<WebSearchSettings accounts={[search]} />);
    await user.click(await screen.findByRole("combobox", { name: "Modelo de Web Search" }));
    await user.click(await screen.findByRole("option", { name: "Sol" }));
    expect(invokeMock).toHaveBeenCalledWith("set_web_search_config", { inheritChat: false, accountAlias: search.alias, model: "gpt-5.6-sol" });
  });
  it("preserva a conta desativada e volta a disponibilizá-la ao reativar", async () => {
    invokeMock.mockResolvedValue({ accountAlias: "openai-codex-pesquisa", model: "gpt-5.6-luna" });
    const active = account("openai-codex-pesquisa");
    const view = render(<WebSearchSettings accounts={[{ ...active, enabled: false }]} />);
    expect(await screen.findByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("Indisponível");
    view.rerender(<WebSearchSettings accounts={[active]} />);
    expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent(active.alias);
    expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).not.toHaveTextContent("Indisponível");
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
  beforeEach(() => { vi.clearAllMocks(); invokeMock.mockReset(); });

  it("começa desligado e oferece apenas contas de provedores compatíveis", async () => {
    invokeMock.mockResolvedValue({ accountAlias: null });
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa"), account("futuro", "unsupported")]} />);
    const select = await screen.findByRole("combobox", { name: "Provedor de Web Search" });
    expect(select).toHaveTextContent("Desligado");
    await user.click(select);
    expect(await screen.findByRole("option", { name: "openai-codex-pesquisa" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "futuro" })).not.toBeInTheDocument();
  });

  it("salva a conta própria da pesquisa, restaura ao reabrir e permite desligar", async () => {
    let selected: string | null = null;
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "set_web_search_config" && args && "accountAlias" in args) selected = args.accountAlias as string | null;
      return { accountAlias: selected };
    });
    const user = userEvent.setup();
    const view = render(<WebSearchSettings accounts={[account("openai-codex-pesquisa"), account("openai-codex-chat")]} />);
    await user.click(await screen.findByRole("combobox", { name: "Provedor de Web Search" }));
    await user.click(await screen.findByRole("option", { name: "openai-codex-pesquisa" }));
    expect(invokeMock).toHaveBeenCalledWith("set_web_search_config", { inheritChat: false, accountAlias: "openai-codex-pesquisa", model: "gpt-5.6-luna" });
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("openai-codex-pesquisa"));
    view.unmount();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa")]} />);
    expect(await screen.findByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("openai-codex-pesquisa");
    await user.click(screen.getByRole("combobox", { name: "Provedor de Web Search" }));
    await user.click(await screen.findByRole("option", { name: "Desligado" }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("Desligado"));
    expect(invokeMock).toHaveBeenCalledWith("set_web_search_config", { inheritChat: false, accountAlias: null, model: null });
  });

  it("bloqueia alterações simultâneas e mantém a seleção após falha ao salvar", async () => {
    let rejectSave!: (error: Error) => void;
    invokeMock.mockResolvedValueOnce({ accountAlias: null }).mockReturnValueOnce(new Promise((_, reject) => { rejectSave = reject; }));
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa")]} />);
    await user.click(await screen.findByRole("combobox", { name: "Provedor de Web Search" }));
    await user.click(await screen.findByRole("option", { name: "openai-codex-pesquisa" }));
    expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).toBeDisabled();
    await act(async () => { rejectSave(new Error("offline")); });
    expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("Desligado");
    expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).toBeEnabled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining("seleção anterior"));
  });

  it("mostra falha de leitura e permite recarregar sem sobrescrever a configuração", async () => {
    invokeMock.mockRejectedValueOnce(new Error("storage")).mockResolvedValueOnce({ accountAlias: "openai-codex-pesquisa", model: "gpt-5.6-luna" });
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[account("openai-codex-pesquisa")]} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível carregar");
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Recarregar Web Search" }));
    expect(await screen.findByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("openai-codex-pesquisa");
    expect(invokeMock).not.toHaveBeenCalledWith("set_web_search_config", expect.anything());
  });

  it("sinaliza uma conta indisponível sem selecionar outra automaticamente", async () => {
    invokeMock.mockResolvedValue({ accountAlias: "openai-codex-ausente" });
    render(<WebSearchSettings accounts={[account("openai-codex-outra")]} />);
    expect(await screen.findByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("openai-codex-ausente · Indisponível");
    expect(screen.getByText(/Conta indisponível/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
