import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { ProviderAccount } from "@/core/provider-accounts";
import { WebSearchSettings } from "./WebSearchSettings";
import { customAccountFixture } from "@/test/custom-provider-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/hooks/use-muse-runtime", () => ({ useMuseRuntime: () => ({ data: null, loading: false, error: null }) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), dismiss: vi.fn() } }));
const invokeMock = vi.mocked(invoke);
const account = (alias: string, providerKind = "openai-codex"): ProviderAccount => ({
  alias, providerKind, enabled: true, models: [{ id: "gpt-5.6-luna", name: "Luna", reasoningLevels: [], defaultReasoningLevel: null }], modelsAvailable: true, createdAt: 1, email: null, accountType: "personal",
});

describe("WebSearchSettings", () => {
  it("selects Brave Search without a model and keeps other tool and chat settings unchanged", async () => {
    const brave: ProviderAccount = { ...account("brave-search-work", "brave-search"), displayName: "Brave Search", models: [] };
    invokeMock.mockImplementation(async (command, args) => command === "get_web_search_config" ? { accountAlias: null, model: null, inheritChat: true } : args);
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[brave, account("chat")]} />);
    await user.click(await screen.findByRole("combobox", { name: "Provedor de Web Search" }));
    await user.click(await screen.findByRole("option", { name: "Brave Search" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_web_search_config", { accountAlias: brave.alias, model: null, inheritChat: false }));
    await screen.findByText(/A busca usa a API Brave Search/);
    expect(screen.queryByRole("combobox", { name: "Modelo de Web Search" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.every(([command]) => command === "get_web_search_config" || command === "set_web_search_config")).toBe(true);
    await user.click(screen.getByRole("combobox", { name: "Provedor de Web Search" }));
    await user.click(await screen.findByRole("option", { name: "Herdar do chat" }));
    await waitFor(() => expect(invokeMock).toHaveBeenLastCalledWith("set_web_search_config", { accountAlias: null, model: null, inheritChat: true }));
  });
  it("keeps disabled Brave selections visible as unavailable without silently switching providers", async () => {
    const brave = { ...account("brave-search-work", "brave-search"), models: [], enabled: false };
    invokeMock.mockResolvedValue({ accountAlias: brave.alias, model: null, inheritChat: false });
    render(<WebSearchSettings accounts={[brave]} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("desativado");
    expect(screen.getByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("brave-search-work · Indisponível");
    expect(invokeMock).not.toHaveBeenCalledWith("set_web_search_config", expect.anything());
  });
  it.each(["vision", "image_generation"] as const)("does not offer Brave Search for %s", async kind => {
    const brave = { ...account("brave-search-work", "brave-search"), models: [] };
    invokeMock.mockResolvedValue({ accountAlias: null, model: null, inheritChat: false });
    const user = userEvent.setup(); render(<WebSearchSettings accounts={[brave]} kind={kind} />);
    await user.click(await screen.findByRole("combobox", { name: `Provedor de ${kind === "vision" ? "Vision" : "Gerar imagens"}` }));
    expect(screen.queryByRole("option", { name: brave.alias })).not.toBeInTheDocument();
  });
  it.each(["custom", "openrouter"])("não oferece modelos de imagens não cadastrados em %s", async providerKind => {
    const router = customAccountFixture(); router.custom!.baseUrl = "https://openrouter.ai/api/v1";
    router.providerKind = providerKind;
    invokeMock.mockImplementation(async (command: string) => command === "refresh_image_generation_models" ? [{ id: "uncatalogued-image", name: "Imagem não cadastrada", reasoningLevels: [], defaultReasoningLevel: null }] : { accountAlias: null, model: null, inheritChat: false });
    const user = userEvent.setup();
    render(<WebSearchSettings kind="image_generation" accounts={[router]} />);
    const select = await screen.findByRole("combobox", { name: "Provedor de Gerar imagens" });
    await waitFor(() => expect(screen.getByRole("button", { name: "Atualizar modelos de imagens" })).toBeEnabled());
    await user.click(select);
    await screen.findByRole("option", { name: "Desligado" });
    expect(screen.queryByRole("option", { name: router.alias })).not.toBeInTheDocument();
  });
  it.each(["custom", "openrouter"])("seleciona Seedream em %s sem alterar o modelo do chat", async providerKind => {
    const models = [
      { id: "bytedance-seed/seedream-5-0-flash", name: "Seedream 5.0 Flash", reasoningLevels: [], defaultReasoningLevel: null },
      { id: "bytedance-seed/seedream-5-0-lite", name: "Seedream 5.0 Lite", reasoningLevels: [], defaultReasoningLevel: null },
    ];
    const router = customAccountFixture();
    router.providerKind = providerKind;
    router.custom = { ...router.custom!, baseUrl: "https://openrouter.ai/api/v1" };
    router.custom.models.push(...models.map(model => ({ ...router.custom!.models[0], ...model, supportsChat: false, supportsImageGeneration: true, supportsTools: false, contextWindow: 0, maxOutputTokens: 0 })));
    invokeMock.mockImplementation(async (command, args) => command === "refresh_image_generation_models" ? models : command === "get_image_generation_config" ? { accountAlias: null, model: null, inheritChat: false } : args);
    const user = userEvent.setup();
    render(<WebSearchSettings kind="image_generation" accounts={[router, account("codex"), account("google", "antigravity")]} />);
    const provider = await screen.findByRole("combobox", { name: "Provedor de Gerar imagens" });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("refresh_image_generation_models"));
    await user.click(provider);
    await user.click(await screen.findByRole("option", { name: router.alias }));
    const model = screen.getByRole("combobox", { name: "Modelo de Gerar imagens" });
    await waitFor(() => expect(model).toBeEnabled());
    await user.click(model);
    await user.click(await screen.findByRole("option", { name: "Seedream 5.0 Lite" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_image_generation_config", { accountAlias: router.alias, model: models[1].id, inheritChat: false }));
    expect(invokeMock.mock.calls.some(([command]) => command === "set_chat_model" || command === "set_vision_config")).toBe(false);
    await waitFor(() => expect(provider).toBeEnabled());
    await user.click(provider);
    await user.click(await screen.findByRole("option", { name: "codex" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_image_generation_config", { accountAlias: "codex", model: "gpt-image-2", inheritChat: false }));
    await waitFor(() => { expect(provider).toBeEnabled(); expect(provider).toHaveTextContent("codex"); });
  });

  it.each(["opencode-go", "charm-hyper"])("selects only %s Vision models confirmed by its live catalog", async providerKind => {
    const gateway = { ...account(`${providerKind}-pessoal`, providerKind), visionModels: ["qwen-vl"], models: [
      { id: "gpt-text", name: "Texto", reasoningLevels: [], defaultReasoningLevel: null },
      { id: "qwen-vl", name: "Qwen Vision", reasoningLevels: [], defaultReasoningLevel: null },
    ] };
    invokeMock.mockImplementation(async (command, args) => command === "get_vision_config" ? { accountAlias: null, model: null, inheritChat: true } : args);
    const user = userEvent.setup();
    render(<WebSearchSettings accounts={[gateway]} kind="vision" />);
    await user.click(await screen.findByRole("combobox", { name: "Provedor de Vision" }));
    await user.click(await screen.findByRole("option", { name: gateway.alias }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_vision_config", { inheritChat: false, accountAlias: gateway.alias, model: "qwen-vl" }));
    const model = screen.getByRole("combobox", { name: "Modelo de Vision" });
    await waitFor(() => expect(model).toBeEnabled());
    await user.click(model);
    expect(await screen.findByRole("option", { name: "Qwen Vision" })).toBeVisible();
    expect(screen.queryByRole("option", { name: "Texto" })).not.toBeInTheDocument();
  });

  it("oferece geração desligada e contas ativas, sem herdar do chat", async () => {
    invokeMock.mockImplementation(async (command, args) => command === "set_image_generation_config" ? args : { accountAlias: null, model: null, inheritChat: false });
    const user = userEvent.setup();
    render(<WebSearchSettings kind="image_generation" accounts={[account("google", "antigravity"), account("codex"), { ...account("inativa", "antigravity"), enabled: false }]} />);
    const select = await screen.findByRole("combobox", { name: "Provedor de Gerar imagens" });
    expect(select).toHaveTextContent("Desligado");
    expect(screen.getByRole("combobox", { name: "Modelo de Gerar imagens" })).toBeDisabled();
    select.focus();
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("option", { name: "Herdar do chat" })).not.toBeInTheDocument();
    expect(await screen.findByRole("option", { name: "codex" })).toBeVisible();
    expect(screen.queryByRole("option", { name: "inativa" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("option", { name: "google" }));
    await waitFor(() => expect(select).toHaveTextContent("google"));
    expect(invokeMock).toHaveBeenCalledWith("set_image_generation_config", { accountAlias: "google", model: "gemini-3.1-flash-image", inheritChat: false });
    await waitFor(() => expect(select).toBeEnabled());
    select.focus();
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("option", { name: "Desligado" }));
    await waitFor(() => expect(select).toHaveTextContent("Desligado"));
  });
  it("disponibiliza Vision Custom pela capacidade declarada, sem presumir pelo nome", async () => {
    const custom = customAccountFixture(); const user = userEvent.setup();
    invokeMock.mockResolvedValue({ accountAlias: null, model: null, inheritChat: true });
    const view = render(<WebSearchSettings accounts={[custom]} kind="vision" />);
    await user.click(await screen.findByRole("combobox", { name: "Provedor de Vision" }));
    expect(await screen.findByRole("option", { name: custom.alias })).toBeVisible();
    await user.keyboard("{Escape}");
    view.rerender(<WebSearchSettings accounts={[{ ...custom, custom: { ...custom.custom!, models: custom.custom!.models.map(model => ({ ...model, supportsImages: false })) } }]} kind="vision" />);
    await user.click(screen.getByRole("combobox", { name: "Provedor de Vision" }));
    await screen.findByRole("option", { name: "Herdar do chat" });
    expect(screen.queryByRole("option", { name: custom.alias })).not.toBeInTheDocument();
  });
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
    expect(screen.getByRole("alert")).toHaveTextContent("O provedor openai-codex-ausente não existe mais");
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Web Search: modelo indisponível", expect.objectContaining({ description: expect.stringContaining("openai-codex-ausente") })));
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
