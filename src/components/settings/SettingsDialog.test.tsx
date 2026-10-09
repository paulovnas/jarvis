import { openUrl } from "@tauri-apps/plugin-opener";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderAccount } from "@/core/provider-accounts";
import SettingsDialog from "./SettingsDialog";
import { coreFixture } from "@/test/core-fixtures";
import { TooltipProvider } from "@/components/ui/tooltip";

const { invokeMock, usageMock, mcpListMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), usageMock: vi.fn(), mcpListMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => command === "get_web_search_config" || command === "get_vision_config" || command === "get_image_generation_config"
    ? Promise.resolve({ accountAlias: null })
    : command === "list_mcp_servers" ? mcpListMock()
    : command === "list_skills" ? Promise.resolve({ includeAgents: false, directory: "/home/.jarvis/skills", skills: [], warnings: [] })
    : command === "list_hooks" ? Promise.resolve({ revision: 0, hooks: [], nativeHooks: [] })
    : command === "list_plugins" ? Promise.resolve({ revision: 0, marketplaces: [], available: [], installed: [], issues: [], appsAccountId: null })
    : command === "get_skill_cache_status" ? Promise.resolve({ bytes: 0, repositories: 0, residues: 0 })
    : command === "get_claude_runtime" ? Promise.resolve({ installed: false, authenticated: false, version: null, models: [], error: null })
    : command === "get_journal_maintenance_status" ? Promise.resolve({ files: 0, conversationJournals: 0, workerJournals: 0, protectedFiles: 0, invalidFiles: 0, candidates: 0, currentBytes: 0, liveBytes: 0, recoverableBytes: 0, obsoleteRecords: 0, maxAmplificationBps: 100 })
    : command === "get_core_status" || command === "check_core_updates" ? Promise.resolve(coreFixture())
    : args === undefined ? invokeMock(command) : invokeMock(command, args),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
}));
vi.mock("@/hooks/use-provider-usage", () => ({ useProviderUsage: usageMock }));

const openUrlMock = vi.mocked(openUrl);
const listeners = new Map<string, Set<EventCallback<unknown>>>();

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function account(
  alias: string,
  overrides: Partial<ProviderAccount> = {},
): ProviderAccount {
  return {
    alias,
    providerKind: "openai-codex",
    enabled: true,
    createdAt: 1_735_689_600,
    email: null,
    accountType: "unknown",
    models: [],
    modelsAvailable: true,
    ...overrides,
  };
}

function renderSettings(onOpenChange = vi.fn()) {
  render(<SettingsDialog open onOpenChange={onOpenChange} />);
  fireEvent.click(screen.getByRole("tab", { name: /Provedores/ }));
  return onOpenChange;
}

describe("SettingsDialog provider accounts", () => {
  it("keeps settings open on outside clicks and Escape, and closes explicitly", async () => {
    invokeMock.mockResolvedValue([]);
    const user = userEvent.setup();
    const changed = vi.fn();
    render(<SettingsDialog open onOpenChange={changed} />);
    await user.keyboard("{Escape}");
    const backdrop = document.querySelector<HTMLElement>('[data-slot="dialog-overlay"]');
    expect(backdrop).not.toBeNull();
    if (backdrop) await user.click(backdrop);
    expect(changed).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Configurações" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(changed).toHaveBeenCalledWith(false);
  });

  it("offers standalone settings without redundant window chrome or saving the main window layout", async () => {
    invokeMock.mockResolvedValue([]);
    const user = userEvent.setup();
    const changed = vi.fn();
    render(<SettingsDialog standalone open onOpenChange={changed} />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Configurações" })).toBeVisible();
    expect(screen.queryByRole("heading", { name: "Configurações" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Fechar Configurações" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    expect(screen.getByRole("tabpanel", { name: /Provedores/ })).toBeVisible();
    expect(invokeMock).not.toHaveBeenCalledWith("save_desktop_layout", expect.anything());
    expect(changed).not.toHaveBeenCalled();
  });

  it("starts native settings in General even when an old tab was persisted", async () => {
    invokeMock.mockResolvedValue([]);
    localStorage.setItem("jarvis:settings-window-tab", "providers");
    const user = userEvent.setup();
    const { unmount } = render(<SettingsDialog standalone open onOpenChange={vi.fn()} />);
    expect(screen.getByRole("tab", { name: "Geral" })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    unmount();
    render(<SettingsDialog standalone open onOpenChange={vi.fn()} />);
    expect(screen.getByRole("tab", { name: "Geral" })).toHaveAttribute("aria-selected", "true");
    expect(invokeMock).not.toHaveBeenCalledWith("save_desktop_layout", expect.anything());
  });

  it("resets a native hook draft on reopening", async () => {
    invokeMock.mockResolvedValue([]);
    const user = userEvent.setup();
    const { unmount } = render(<SettingsDialog standalone open onOpenChange={vi.fn()} />);
    await user.click(screen.getByRole("tab", { name: "Hooks" }));
    expect(await screen.findByRole("button", { name: "Adicionar hook" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Adicionar hook" }));
    await user.type(screen.getByRole("textbox", { name: "Nome do hook" }), "Rascunho");
    unmount();
    render(<SettingsDialog standalone open onOpenChange={vi.fn()} />);
    expect(screen.getByRole("tab", { name: "Geral" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("textbox", { name: "Nome do hook" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Hooks" }));
    expect(await screen.findByRole("button", { name: "Adicionar hook" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Adicionar hook" }));
    expect(screen.getByRole("textbox", { name: "Nome do hook" })).toHaveValue("");
    expect(invokeMock).not.toHaveBeenCalledWith("save_desktop_layout", expect.anything());
  });

  it("waits for a hook mutation before accepting native window close", async () => {
    const pending = deferred<unknown>();
    const requestClose: { current: (() => void) | null } = { current: null };
    const closed = vi.fn();
    invokeMock.mockImplementation(command => command === "save_hook" ? pending.promise : Promise.resolve([]));
    const user = userEvent.setup();
    render(<SettingsDialog standalone open onOpenChange={closed} onCloseRequestChange={handler => { requestClose.current = handler; }} />);
    await user.click(screen.getByRole("tab", { name: "Hooks" }));
    await user.click(await screen.findByRole("button", { name: "Adicionar hook" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Nome do hook" }), { target: { value: "Validar" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Comando" }), { target: { value: "node validate.mjs" } });
    await user.click(screen.getByRole("button", { name: "Salvar hook" }));
    act(() => requestClose.current?.());
    expect(closed).not.toHaveBeenCalled();
    await act(async () => { pending.resolve({ revision: 1, hooks: [], nativeHooks: [] }); });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    act(() => requestClose.current?.());
    expect(closed).toHaveBeenCalledWith(false);
  });

  it("resets the native plugin search on reopening", async () => {
    invokeMock.mockResolvedValue([]);
    const user = userEvent.setup();
    const { unmount } = render(<SettingsDialog standalone open onOpenChange={vi.fn()} />);
    await user.click(screen.getByRole("tab", { name: "Plugins" }));
    expect(await screen.findByRole("textbox", { name: "Buscar plugins" })).toBeVisible();
    await user.type(screen.getByRole("textbox", { name: "Buscar plugins" }), "firebase");
    unmount();
    render(<SettingsDialog standalone open onOpenChange={vi.fn()} />);
    expect(screen.getByRole("tab", { name: "Geral" })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("tab", { name: "Plugins" }));
    expect(await screen.findByRole("textbox", { name: "Buscar plugins" })).toHaveValue("");
    expect(invokeMock).not.toHaveBeenCalledWith("save_desktop_layout", expect.anything());
  });

  it.each([false, true])("discards temporary provider state across open changes (standalone: %s) and retains saved accounts", async standalone => {
    const saved = account("openai-codex-pessoal");
    invokeMock.mockImplementation(command => Promise.resolve(command === "list_provider_accounts" ? [saved] : undefined));
    const user = userEvent.setup();
    const changed = vi.fn();
    const props = { standalone, onOpenChange: changed };
    const { rerender } = render(<SettingsDialog {...props} open />);
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "rascunho");
    rerender(<SettingsDialog {...props} open={false} />);
    expect(screen.queryByRole("textbox", { name: "Sufixo do alias" })).not.toBeInTheDocument();
    rerender(<SettingsDialog {...props} open />);
    expect(screen.getByRole("tab", { name: "Geral" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("dialog", { name: "Adicionar conta" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    expect(await screen.findByRole("button", { name: `Detalhes de ${saved.alias}` })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Adicionar conta" }));
    expect(screen.getByRole("textbox", { name: "Sufixo do alias" })).toHaveValue("");
    expect(screen.getByRole("combobox", { name: "Provedor" })).toHaveTextContent("OpenAI Codex");
    expect(invokeMock).not.toHaveBeenCalledWith("save_desktop_layout", expect.anything());
  });

  it("waits for a prepared plugin apply before accepting native window close", async () => {
    const pending = deferred<unknown>();
    const requestClose: { current: (() => void) | null } = { current: null };
    const closed = vi.fn();
    invokeMock.mockImplementation(command => command === "apply_plugin_change" ? pending.promise : command === "preview_plugin_change" ? Promise.resolve({ receiptId: "plugin-review", revision: 0, preview: { title: "Revisar marketplace", description: "Adicionar origem", source: "https://github.com/org/repo.git", hash: "", components: [], commands: [], requirements: [], warnings: [], affectedIds: [] } }) : Promise.resolve([]));
    const user = userEvent.setup();
    render(<SettingsDialog standalone open onOpenChange={closed} onCloseRequestChange={handler => { requestClose.current = handler; }} />);
    await user.click(screen.getByRole("tab", { name: "Plugins" }));
    const add = await screen.findByRole("button", { name: "Adicionar" }); await waitFor(() => expect(add).toBeEnabled()); await user.click(add);
    await user.click(await screen.findByRole("menuitem", { name: "Marketplace" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Origem do marketplace" }), { target: { value: "org/repo" } });
    await user.click(screen.getByRole("button", { name: "Revisar marketplace" }));
    await user.click(await screen.findByRole("button", { name: "Confirmar alteração" }));
    act(() => requestClose.current?.()); expect(closed).not.toHaveBeenCalled();
    await act(async () => { pending.resolve({ revision: 1, marketplaces: [], available: [], installed: [], issues: [], appsAccountId: null }); });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    act(() => requestClose.current?.()); expect(closed).toHaveBeenCalledWith(false);
  });

  it("cancels pending OAuth before accepting a native window close request", async () => {
    const existing = account("openai-codex-pessoal");
    const pending = deferred<ProviderAccount>();
    const requestClose: { current: (() => void) | null } = { current: null };
    const changed = vi.fn();
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_provider_accounts") return Promise.resolve([existing]);
      if (command === "reauthorize_provider_account") return Promise.resolve({ flowId: "native-oauth", authorizationUrl: "https://example.test/auth" });
      if (command === "wait_openai_codex_connection") return pending.promise;
      if (command === "cancel_openai_codex_connection") pending.reject({ code: "cancelled", message: "Cancelada" });
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<SettingsDialog standalone open onOpenChange={changed} onCloseRequestChange={handler => { requestClose.current = handler; }} />);
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${existing.alias}` }));
    await user.click(screen.getByRole("button", { name: "Re-autorizar" }));
    await screen.findByText("Aguardando autenticação no navegador");
    expect(changed).not.toHaveBeenCalled();
    expect(requestClose.current).not.toBeNull();
    // This callback is installed on the native title bar close request.
    await act(async () => { requestClose.current?.(); });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cancel_openai_codex_connection", { flowId: "native-oauth" }));
    await waitFor(() => expect(changed).toHaveBeenCalledWith(false));
  });

  it.each(["openai-codex", "opencode-go"] as const)("persists independent %s quota windows and keeps choices across the master switch", async providerKind => {
    const user = userEvent.setup();
    const errorToast = vi.spyOn(toast, "error");
    const provider = account(`${providerKind}-pessoal`, { providerKind });
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_provider_accounts") return Promise.resolve([provider]);
      if (command === "get_provider_model_references") return Promise.resolve({ references: [], bindings: [] });
      return Promise.resolve(undefined);
    });
    const changed = vi.fn();
    render(<SettingsDialog open onOpenChange={vi.fn()} onAccountsChange={changed} />);
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${provider.alias}` }));
    const fiveHour = screen.getByRole("checkbox", { name: "5 horas" });
    const weekly = screen.getByRole("checkbox", { name: "Semanal" });
    expect(fiveHour).toBeChecked();
    expect(weekly).toBeChecked();
    await user.click(fiveHour);
    await waitFor(() => expect(fiveHour).not.toBeChecked());
    expect(invokeMock).toHaveBeenCalledWith("set_provider_usage_visibility", { alias: provider.alias, showUsage: true, showThirdPartyUsage: false, showFiveHourUsage: false, showWeeklyUsage: true });
    await user.click(weekly);
    await waitFor(() => expect(weekly).not.toBeChecked());
    expect(invokeMock).toHaveBeenCalledWith("set_provider_usage_visibility", { alias: provider.alias, showUsage: true, showThirdPartyUsage: false, showFiveHourUsage: false, showWeeklyUsage: false });
    await user.click(screen.getByRole("switch", { name: `Limites de ${provider.alias} na statusbar` }));
    await waitFor(() => expect(fiveHour).toHaveAttribute("aria-disabled", "true"));
    expect(changed).toHaveBeenLastCalledWith([expect.objectContaining({ enabled: true, showUsage: false, showFiveHourUsage: false, showWeeklyUsage: false })]);
    expect(invokeMock).toHaveBeenLastCalledWith("set_provider_usage_visibility", { alias: provider.alias, showUsage: false, showThirdPartyUsage: false });
    await user.click(screen.getByRole("switch", { name: `Limites de ${provider.alias} na statusbar` }));
    await waitFor(() => expect(fiveHour).not.toHaveAttribute("aria-disabled", "true"));
    expect(fiveHour).not.toBeChecked();
    expect(weekly).not.toBeChecked();
    invokeMock.mockRejectedValueOnce(new Error("storage unavailable"));
    await user.click(weekly);
    await waitFor(() => expect(errorToast).toHaveBeenCalledWith("Não foi possível salvar a visualização dos limites."));
    expect(weekly).not.toBeChecked();
  }, 15_000);

  it("adds Go by subscription key, refreshes its models and replaces its key without OAuth", async () => {
    const user = userEvent.setup();
    const go = account("opencode-go-pessoal", { providerKind: "opencode-go", models: [{ id: "glm", name: "GLM", reasoningLevels: [], defaultReasoningLevel: null }] });
    const refreshed = { ...go, models: [{ id: "kimi", name: "Kimi", reasoningLevels: [], defaultReasoningLevel: null }] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_provider_accounts") return Promise.resolve([]);
      if (command === "get_provider_model_references") return Promise.resolve({ references: [], bindings: [] });
      if (command === "save_opencode_go_provider") return Promise.resolve(go);
      if (command === "refresh_provider_models") return Promise.resolve([refreshed]);
      return Promise.resolve(undefined);
    });
    const changed = vi.fn();
    render(<SettingsDialog open onOpenChange={vi.fn()} onAccountsChange={changed} />);
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    await user.click(screen.getByRole("button", { name: "Adicionar conta" }));
    screen.getByRole("combobox", { name: "Provedor" }).focus();
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("option", { name: "OpenCode Go" }));
    fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
    fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: "private-test-key" } });
    await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
    const detailsButton = await screen.findByRole("button", { name: `Detalhes de ${go.alias}` });
    expect(invokeMock).toHaveBeenCalledWith("save_opencode_go_provider", { alias: go.alias, apiKey: "private-test-key", editing: false });
    await user.click(detailsButton);
    expect(screen.getByRole("switch", { name: `Limites de ${go.alias} na statusbar` })).toBeChecked();
    expect(screen.queryByRole("button", { name: "Re-autorizar" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Atualizar modelos" }));
    expect(await screen.findByText("Kimi")).toBeVisible();
    expect(changed).toHaveBeenLastCalledWith([refreshed]);
    await user.click(screen.getByRole("button", { name: "Atualizar chave" }));
    expect(await screen.findByRole("dialog", { name: "Editar OpenCode Go" })).toBeVisible();
    expect(screen.getByLabelText("Sufixo do alias")).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: "replacement-test-key" } });
    await user.click(screen.getByRole("button", { name: "Salvar alterações" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("save_opencode_go_provider", { alias: go.alias, apiKey: "replacement-test-key", editing: true }));
    expect(invokeMock).not.toHaveBeenCalledWith("connect_provider_account", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("reauthorize_provider_account", expect.anything());
  });
  it("persists model availability and replaces the catalog from the provider without reauthentication", async () => {
    const user = userEvent.setup();
    const oldModel = { id: "old", name: "Antigo", reasoningLevels: [], defaultReasoningLevel: null };
    const newModel = { id: "gpt-6-sol", name: "GPT 6 Sol", reasoningLevels: [], defaultReasoningLevel: null };
    const provider = account("openai-codex-pessoal", { models: [oldModel] });
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_provider_accounts") return Promise.resolve([provider]);
      if (command === "get_provider_model_references") return Promise.resolve({ references: [], bindings: [] });
      if (command === "set_provider_model_enabled") return Promise.resolve(["old"]);
      if (command === "refresh_provider_models") return Promise.resolve([{ ...provider, models: [newModel], disabledModels: ["old"] }]);
      return Promise.resolve(undefined);
    });
    const changed = vi.fn();
    render(<SettingsDialog open onOpenChange={vi.fn()} onAccountsChange={changed} />);
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${provider.alias}` }));
    await user.click(screen.getByRole("switch", { name: "Disponibilizar Antigo" }));
    await waitFor(() => expect(screen.getByRole("switch", { name: "Disponibilizar Antigo" })).not.toBeChecked());
    expect(invokeMock).toHaveBeenCalledWith("set_provider_model_enabled", { alias: provider.alias, modelId: "old", enabled: false });
    await user.click(screen.getByRole("button", { name: "Atualizar modelos" }));
    expect(await screen.findByText("GPT 6 Sol")).toBeVisible();
    expect(screen.queryByText("Antigo")).not.toBeInTheDocument();
    expect(changed).toHaveBeenLastCalledWith([expect.objectContaining({ models: [newModel], disabledModels: ["old"] })]);
    expect(invokeMock).not.toHaveBeenCalledWith("reauthorize_provider_account", expect.anything());
  });
  it("does not repeat visible settings labels in a tooltip", async () => {
    const user = userEvent.setup();
    invokeMock.mockResolvedValue([]);
    render(<TooltipProvider delay={0}><SettingsDialog open onOpenChange={vi.fn()} /></TooltipProvider>);
    const providers = screen.getByRole("tab", { name: /Provedores/ });
    await user.hover(providers);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });
  it("keeps settings labels available when the navigation collapses to icons", async () => {
    vi.spyOn(window, "matchMedia").mockReturnValue({
      matches: true,
      media: "(max-width: 639px)",
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    });
    const user = userEvent.setup();
    invokeMock.mockResolvedValue([]);
    render(<TooltipProvider delay={0}><SettingsDialog open onOpenChange={vi.fn()} /></TooltipProvider>);
    await waitFor(() => expect(screen.getByRole("tab", { name: "Geral" })).toHaveFocus());
    const providers = screen.getByRole("tab", { name: /Provedores/ });
    await user.hover(providers);
    await waitFor(() => expect(screen.getByRole("tooltip")).toHaveTextContent("Provedores"));
  }, 15_000);
  it("identifica a aba ativa e mantém a navegação disponível ao trocar o conteúdo", async () => {
    const user = userEvent.setup();
    invokeMock.mockResolvedValue([]);
    renderSettings();
    const navigation = screen.getByRole("tablist", { name: "Configurações" });
    expect(navigation).toHaveAttribute("aria-orientation", "vertical");
    const providers = within(navigation).getByRole("tab", { name: /Provedores/ });
    expect(providers).toHaveAttribute("aria-selected", "true");
    expect(providers).toHaveAttribute("data-active");
    const skills = within(navigation).getByRole("tab", { name: /Skills/ });
    await user.click(skills);
    expect(skills).toHaveAttribute("data-active");
    expect(skills).toHaveAttribute("aria-selected", "true");
    expect(providers).not.toHaveAttribute("data-active");
    expect(navigation).toBeVisible();
    expect(screen.getAllByRole("tab")).toHaveLength(12);
    await user.click(screen.getByRole("tab", { name: "Geral" }));
    expect(screen.queryByRole("region", { name: "Core" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Ferramentas" }));
    expect(await screen.findByRole("region", { name: "Core" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "Ferramentas" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowUp}{ArrowUp}{ArrowUp}{ArrowUp}{ArrowUp}{Enter}");
    expect(screen.getByRole("tab", { name: "Geral" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { name: "Geral", level: 2 })).toBeVisible();
  });
  it("persiste a visibilidade dos limites sem desativar a conta e preserva a preferência se o salvamento falhar", async () => {
    const user = userEvent.setup();
    const errorToast = vi.spyOn(toast, "error");
    const google = account("antigravity-pessoal", { providerKind: "antigravity" });
    invokeMock.mockImplementation((command: string) => Promise.resolve(command === "list_provider_accounts" ? [google] : undefined));
    const changed = vi.fn();
    render(<SettingsDialog open onOpenChange={vi.fn()} onAccountsChange={changed} />);
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${google.alias}` }));
    const usage = screen.getByRole("switch", { name: `Limites de ${google.alias} na statusbar` });
    expect(usage).toBeChecked();
    await user.click(usage);
    await waitFor(() => expect(usage).not.toBeChecked());
    expect(invokeMock).toHaveBeenCalledWith("set_provider_usage_visibility", { alias: google.alias, showUsage: false, showThirdPartyUsage: false });
    expect(changed).toHaveBeenLastCalledWith([expect.objectContaining({ enabled: true, showUsage: false })]);
    invokeMock.mockRejectedValueOnce(new Error("storage unavailable"));
    await user.click(screen.getByRole("switch", { name: `Incluir modelos de terceiros de ${google.alias}` }));
    await waitFor(() => expect(errorToast).toHaveBeenCalledWith("Não foi possível salvar a visualização dos limites."));
    expect(screen.getByRole("switch", { name: `Incluir modelos de terceiros de ${google.alias}` })).not.toBeChecked();
  });
  it("persiste o alerta de limite dentro do provedor usando somente janelas disponíveis", async () => {
    const user = userEvent.setup();
    const codex = account("openai-codex-pessoal");
    usageMock.mockReturnValue({ data: { alias: codex.alias, fetchedAt: 1, email: null, plan: null, error: null, resetCredits: null, windows: [
      { id: "weekly", group: "Codex", thirdParty: false, label: "7d", durationSeconds: 604_800, remainingPercent: 80, resetsAt: 2 },
    ] }, error: false });
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_provider_accounts") return Promise.resolve([codex]);
      return Promise.resolve(undefined);
    });
    const changed = vi.fn();
    render(<SettingsDialog open onOpenChange={vi.fn()} onAccountsChange={changed} />);
    await user.click(screen.getByRole("tab", { name: /Provedores/ }));
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${codex.alias}` }));
    const alert = await screen.findByRole("switch", { name: "Alertar sobre limite" });
    await waitFor(() => expect(alert).toBeEnabled());
    await user.click(alert);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_provider_usage_alert", { alias: codex.alias, alert: { window: "weekly", remainingPercent: 20 } }));
    expect(changed).toHaveBeenLastCalledWith([expect.objectContaining({ usageAlert: { window: "weekly", remainingPercent: 20 } })]);
  });
  it("conecta Antigravity pelo navegador e mostra seus modelos no mesmo card", async () => {
    const user = userEvent.setup();
    const google = account("antigravity-pessoal", { providerKind: "antigravity", email: "google@example.com", models: [{ id: "gemini-pro", name: "Gemini Pro", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" }] });
    const login = deferred<ProviderAccount>(); let connected = false;
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_provider_accounts") return Promise.resolve(connected ? [google] : []);
      if (command === "begin_openai_codex_connection") return Promise.resolve({ flowId: "google-flow", authorizationUrl: "https://accounts.google.com/o/oauth2/v2/auth?state=test" });
      if (command === "wait_openai_codex_connection") return login.promise;
      return Promise.resolve(undefined);
    });
    renderSettings();
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    expect(screen.getByRole("combobox", { name: "Provedor" })).toHaveTextContent("OpenAI Codex");
    await user.click(screen.getByRole("combobox", { name: "Provedor" }));
    await user.click(await screen.findByRole("option", { name: "Antigravity" }));
    expect(screen.getByRole("combobox", { name: "Provedor" })).toHaveTextContent("Antigravity");
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "pessoal");
    await user.click(screen.getByRole("button", { name: "Conectar com Antigravity" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("begin_openai_codex_connection", { alias: "antigravity-pessoal" }));
    expect(openUrlMock).toHaveBeenCalledWith("https://accounts.google.com/o/oauth2/v2/auth?state=test");
    expect(screen.getByRole("dialog", { name: "Conectar com Antigravity" })).toBeInTheDocument();
    connected = true; login.resolve(google);
    const card = await screen.findByRole("button", { name: "Detalhes de antigravity-pessoal" });
    expect(card).toHaveTextContent("Antigravity");
    await user.click(card);
    expect(await screen.findByText("Gemini Pro")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: /Ativar antigravity-pessoal/ })).toBeChecked();
    expect(screen.getByRole("button", { name: "Desconectar" })).toBeInTheDocument();
  });
  beforeEach(() => {
    localStorage.removeItem("jarvis:settings-window-tab");
    vi.restoreAllMocks();
    invokeMock.mockReset();
    mcpListMock.mockReset().mockResolvedValue([]);
    listeners.clear();
    vi.mocked(listen).mockImplementation(async (name, callback) => {
      const callbacks = listeners.get(name) ?? new Set(); callbacks.add(callback); listeners.set(name, callbacks);
      return () => { callbacks.delete(callback); };
    });
    usageMock.mockReset().mockReturnValue({ data: null, error: false });
    openUrlMock.mockReset();
    openUrlMock.mockResolvedValue(undefined);
  });

  it("ordena as abas, apresenta Core em Ferramentas e mantém Skills disponível", async () => {
    invokeMock.mockResolvedValueOnce([account("openai-codex-pessoal")]);
    const user = userEvent.setup();
    render(<SettingsDialog open onOpenChange={vi.fn()} />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map(tab => tab.textContent?.replace(/\\d/g, ""))).toEqual(["Geral", "Terminal", "Navegador", "Jarvis Voice", "Espaços", "Ferramentas", "Fluxos", "Provedores", "Skills", "MCPs", "Hooks", "Plugins"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(within(screen.getByRole("tabpanel", { name: "Geral" })).queryByRole("heading", { name: "Core" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Ferramentas" }));
    expect(await within(screen.getByRole("tabpanel", { name: "Ferramentas" })).findByRole("heading", { name: "Core" })).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Skills/ }));
    expect(await screen.findByRole("button", { name: "Marketplace" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("tab", { name: /Skills/ })).toHaveTextContent("0"));
    await waitFor(() => expect(screen.getByRole("tab", { name: /MCPs/ })).toHaveTextContent("0"));
    expect(screen.getByRole("tab", { name: /Provedores/ })).toHaveTextContent("2");
  });

  it("refreshes the MCP navigation count after an agent adds a server while the MCP tab is closed", async () => {
    invokeMock.mockResolvedValue([]);
    const server = { id: "docs", name: "docs", kind: "remote", enabled: false, configured: true, revision: 1, lastCheck: null };
    mcpListMock.mockResolvedValueOnce([server]).mockResolvedValue([server, { ...server, id: "docs-2", name: "docs-2" }]);
    const { unmount } = render(<SettingsDialog open onOpenChange={vi.fn()} />);
    const tab = screen.getByRole("tab", { name: /MCPs/ });
    await waitFor(() => expect(tab).toHaveTextContent("1"));
    expect(tab).toHaveAttribute("aria-selected", "false");
    await act(async () => { listeners.get("mcp-servers:changed")?.forEach(callback => callback({ event: "mcp-servers:changed", id: 1, payload: null })); });
    await waitFor(() => expect(tab).toHaveTextContent("2"));
    expect(tab).toHaveAttribute("aria-selected", "false");
    expect(screen.getByRole("tabpanel", { name: "Geral" })).toBeVisible();
    unmount();
    await waitFor(() => expect(listeners.get("mcp-servers:changed")?.size).toBe(0));
  });

  it("keeps the latest MCP count when an older list request finishes after a change event", async () => {
    invokeMock.mockResolvedValue([]);
    const initial = deferred<unknown>();
    const server = { id: "docs", name: "docs", kind: "remote", enabled: false, configured: true, revision: 1, lastCheck: null };
    mcpListMock.mockReturnValueOnce(initial.promise).mockResolvedValue([server, { ...server, id: "docs-2", name: "docs-2" }]);
    render(<SettingsDialog open onOpenChange={vi.fn()} />);
    await waitFor(() => expect(mcpListMock).toHaveBeenCalledOnce());
    await act(async () => { listeners.get("mcp-servers:changed")?.forEach(callback => callback({ event: "mcp-servers:changed", id: 1, payload: null })); });
    const tab = screen.getByRole("tab", { name: /MCPs/ });
    await waitFor(() => expect(tab).toHaveTextContent("2"));
    await act(async () => { initial.resolve([server]); });
    expect(tab).toHaveTextContent("2");
  });

  it("fecha o formulário sem fechar o drawer e restaura o foco", async () => {
    invokeMock.mockResolvedValueOnce([]);
    const user = userEvent.setup();
    const changed = renderSettings();
    const add = await screen.findByRole("button", { name: "Adicionar conta" });
    await user.click(add);
    const dialog = screen.getByRole("dialog", { name: "Adicionar conta" });
    expect(within(dialog).getByRole("textbox", { name: "Sufixo do alias" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Adicionar conta" })).not.toBeInTheDocument());
    expect(changed).not.toHaveBeenCalledWith(false);
    expect(screen.getByRole("tab", { name: /Provedores/ })).toHaveAttribute("aria-selected", "true");
  });

  it("mostra o skeleton ao abrir e depois o estado vazio", async () => {
    const list = deferred<ProviderAccount[]>();
    invokeMock.mockReturnValueOnce(list.promise);

    renderSettings();

    expect(screen.getByRole("status", { name: /carregando contas/i })).toBeInTheDocument();
    list.resolve([]);

    expect(await screen.findByRole("button", { name: "Detalhes de Claude Code" })).toBeInTheDocument();
    expect(screen.getAllByTestId("provider-account-claude-code")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Adicionar conta" })).toBeInTheDocument();
    expect(await screen.findByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("Desligado");
  });

  it("mostra os dados da assinatura e os modelos ao abrir a modal do card", async () => {
    const user = userEvent.setup();
    const connected = account("openai-codex-empresa", {
      email: "dev@empresa.com",
      accountType: "enterprise",
      models: [
        { id: "gpt-5.6-luna", name: "GPT-5.6 Luna", reasoningLevels: ["medium", "xhigh"], defaultReasoningLevel: "medium" },
        { id: "gpt-5.6-sol", name: "GPT-5.6 Sol", reasoningLevels: [], defaultReasoningLevel: null },
      ],
    });
    invokeMock.mockResolvedValueOnce([connected]);

    renderSettings();

    const card = await screen.findByTestId(`provider-account-${connected.alias}`);
    expect(within(card).getByText("OpenAI Codex · 2 modelos ativos")).toBeInTheDocument();
    expect(within(card).queryByText("dev@empresa.com")).not.toBeInTheDocument();
    expect(within(card).queryByText("GPT-5.6 Luna")).not.toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: `Detalhes de ${connected.alias}` }));
    const details = screen.getByRole("dialog", { name: connected.alias });
    expect(within(details).getByText("dev@empresa.com")).toBeInTheDocument();
    expect(within(details).getByText("Enterprise")).toBeInTheDocument();
    expect(within(details).getByText("GPT-5.6 Luna")).toBeInTheDocument();
    expect(within(details).getByText("GPT-5.6 Sol")).toBeInTheDocument();
  });

  it.each(["openai-codex", "antigravity"] as const)("reautoriza %s dentro dos detalhes e atualiza a conta sem recriar o cadastro", async providerKind => {
    const user = userEvent.setup();
    const existing = account(`${providerKind}-pessoal`, { providerKind, modelsAvailable: false, showUsage: false });
    const renewed = { ...existing, modelsAvailable: true, email: "dev@example.com", models: [{ id: "model-new", name: "Modelo do novo plano", reasoningLevels: [], defaultReasoningLevel: null }] };
    const wait = deferred<ProviderAccount>();
    const authorizationUrl = "https://example.test/reauthorize";
    let lists = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_provider_accounts") return Promise.resolve(lists++ === 0 ? [existing] : [renewed]);
      if (command === "reauthorize_provider_account") return Promise.resolve({ flowId: "reauth-flow", authorizationUrl });
      if (command === "wait_openai_codex_connection") return wait.promise;
      if (command === "get_provider_model_references") return Promise.resolve({ references: [], bindings: [] });
      return Promise.resolve(undefined);
    });
    const success = vi.spyOn(toast, "success");
    renderSettings();
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${existing.alias}` }));
    await user.click(within(screen.getByRole("dialog", { name: existing.alias })).getByRole("button", { name: "Re-autorizar" }));
    expect(await screen.findByText("Aguardando autenticação no navegador")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Sufixo do alias" })).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("reauthorize_provider_account", { alias: existing.alias });
    expect(openUrlMock).toHaveBeenCalledWith(authorizationUrl);
    wait.resolve(renewed);
    await waitFor(() => expect(success).toHaveBeenCalledWith("Autorização atualizada"));
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${existing.alias}` }));
    const details = screen.getByRole("dialog", { name: existing.alias });
    expect(within(details).getByText("Modelo do novo plano")).toBeInTheDocument();
    expect(within(details).getByRole("switch", { name: `Limites de ${existing.alias} na statusbar` })).not.toBeChecked();
    expect(invokeMock).not.toHaveBeenCalledWith("disconnect_provider_account_command", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("begin_openai_codex_connection", expect.anything());
  });

  it.each(["failure", "cancel"])("preserva o provedor após %s na reautorização e permite repetir o login", async outcome => {
    const user = userEvent.setup();
    const existing = account("openai-codex-pessoal", { enabled: false, modelsAvailable: false });
    let pending = deferred<ProviderAccount>();
    let attempts = 0;
    invokeMock.mockImplementation(command => {
      if (command === "list_provider_accounts") return Promise.resolve([existing]);
      if (command === "reauthorize_provider_account") return Promise.resolve({ flowId: `renew-${++attempts}`, authorizationUrl: "https://example.test/auth" });
      if (command === "wait_openai_codex_connection") return pending.promise;
      if (command === "cancel_openai_codex_connection") pending.reject({ code: "cancelled", message: "Cancelada" });
      return Promise.resolve();
    });
    renderSettings();
    await user.click(await screen.findByRole("button", { name: `Detalhes de ${existing.alias}` }));
    await user.click(screen.getByRole("button", { name: "Re-autorizar" }));
    await screen.findByText("Aguardando autenticação no navegador");
    if (outcome === "cancel") await user.click(screen.getByRole("button", { name: "Cancelar conexão" }));
    else pending.reject({ code: "token_exchange", message: "Não foi possível atualizar a autorização." });
    const retry = await screen.findByRole("button", { name: "Tentar novamente" });
    if (outcome === "failure") expect(screen.getByRole("alert")).toHaveTextContent("Não foi possível atualizar a autorização.");
    pending = deferred<ProviderAccount>();
    await user.click(retry);
    await screen.findByText("Aguardando autenticação no navegador");
    expect(attempts).toBe(2);
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(await screen.findByTestId(`provider-account-${existing.alias}`)).toHaveTextContent("Desativada");
    expect(invokeMock).not.toHaveBeenCalledWith("disconnect_provider_account_command", expect.anything());
  });

  it("permite tentar novamente após falha ao carregar contas", async () => {
    const connected = account("openai-codex-pessoal");
    const user = userEvent.setup();
    invokeMock
      .mockRejectedValueOnce({
        code: "database",
        message: "Não foi possível acessar as contas conectadas.",
      })
      .mockResolvedValueOnce([connected]);

    renderSettings();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Não foi possível acessar as contas conectadas.",
    );
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(await screen.findByText(connected.alias)).toBeInTheDocument();
  });

  it("valida o sufixo imediatamente e bloqueia aliases inválidos", async () => {
    const user = userEvent.setup();
    invokeMock.mockResolvedValueOnce([]);
    renderSettings();

    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    const input = screen.getByRole("textbox", { name: "Sufixo do alias" });
    await user.type(input, "Pessoal");

    expect(screen.getByRole("alert")).toHaveTextContent(/letras minúsculas/i);
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));
    expect(invokeMock).not.toHaveBeenCalledWith(
      "begin_openai_codex_connection",
      expect.anything(),
    );
  });

  it("inicia OAuth, reabre a mesma URL e atualiza a lista ao concluir", async () => {
    const connected = account("openai-codex-pessoal");
    const wait = deferred<ProviderAccount>();
    const user = userEvent.setup();
    const authorizationUrl = "https://auth.openai.com/oauth/authorize?state=test";
    invokeMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ flowId: "flow-1", authorizationUrl })
      .mockReturnValueOnce(wait.promise)
      .mockResolvedValueOnce([connected]);
    const successSpy = vi.spyOn(toast, "success");

    renderSettings();
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "pessoal");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));

    expect(await screen.findByText("Aguardando autenticação no navegador")).toBeInTheDocument();
    expect(openUrlMock).toHaveBeenCalledWith(authorizationUrl);
    await user.click(screen.getByRole("button", { name: "Abrir navegador novamente" }));
    expect(openUrlMock).toHaveBeenCalledTimes(2);
    expect(openUrlMock).toHaveBeenNthCalledWith(2, authorizationUrl);
    expect(invokeMock).toHaveBeenCalledWith("begin_openai_codex_connection", {
      alias: "openai-codex-pessoal",
    });
    expect(invokeMock).toHaveBeenCalledWith("wait_openai_codex_connection", {
      flowId: "flow-1",
    });

    wait.resolve(connected);
    await waitFor(() => expect(successSpy).toHaveBeenCalledWith("Conta conectada"));
    expect(await screen.findByText(connected.alias)).toBeInTheDocument();
  });

  it("mantém o formulário após falha e permite tentar novamente", async () => {
    const user = userEvent.setup();
    invokeMock
      .mockResolvedValueOnce([])
      .mockRejectedValueOnce({ code: "duplicate_account", message: "Este alias já está conectado." });

    renderSettings();
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "pessoal");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Este alias já está conectado.");
    expect(screen.getByRole("button", { name: "Tentar novamente" })).toBeEnabled();
    expect(screen.getByRole("textbox", { name: "Sufixo do alias" })).toHaveValue("pessoal");
  });
  it("libera o fluxo quando o navegador falha e permite tentar novamente", async () => {
    const user = userEvent.setup();
    const wait = deferred<ProviderAccount>();
    const connected = account("openai-codex-pessoal");
    const firstUrl = "https://example.test/first";
    const retryUrl = "https://example.test/retry";
    invokeMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ flowId: "flow-open-failure", authorizationUrl: firstUrl })
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce({ flowId: "flow-open-retry", authorizationUrl: retryUrl })
      .mockReturnValueOnce(wait.promise)
      .mockResolvedValueOnce([connected]);
    openUrlMock.mockRejectedValueOnce(new Error("browser unavailable"));

    renderSettings();
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "pessoal");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível abrir o navegador.");
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(await screen.findByText("Aguardando autenticação no navegador")).toBeInTheDocument();
    expect(openUrlMock).toHaveBeenNthCalledWith(2, retryUrl);

    wait.resolve(connected);
    expect(await screen.findByText(connected.alias)).toBeInTheDocument();
  });

  it("libera o fluxo quando a espera falha e permite tentar novamente", async () => {
    const user = userEvent.setup();
    const wait = deferred<ProviderAccount>();
    const retryWait = deferred<ProviderAccount>();
    const connected = account("openai-codex-pessoal");
    const firstUrl = "https://example.test/first";
    const retryUrl = "https://example.test/retry";
    invokeMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ flowId: "flow-wait-failure", authorizationUrl: firstUrl })
      .mockReturnValueOnce(wait.promise)
      .mockResolvedValueOnce({ flowId: "flow-wait-retry", authorizationUrl: retryUrl })
      .mockReturnValueOnce(retryWait.promise)
      .mockResolvedValueOnce([connected]);

    renderSettings();
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "pessoal");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));
    expect(await screen.findByText("Aguardando autenticação no navegador")).toBeInTheDocument();

    wait.reject({ code: "oauth_wait_failed", message: "A autenticação expirou." });
    expect(await screen.findByRole("alert")).toHaveTextContent("A autenticação expirou.");
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(await screen.findByText("Aguardando autenticação no navegador")).toBeInTheDocument();
    expect(openUrlMock).toHaveBeenNthCalledWith(2, retryUrl);

    retryWait.resolve(connected);
    expect(await screen.findByText(connected.alias)).toBeInTheDocument();
  });


  it("cancela uma conexão pendente e retorna ao formulário", async () => {
    const user = userEvent.setup();
    const wait = deferred<ProviderAccount>();
    const cancel = deferred<void>();
    invokeMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ flowId: "flow-2", authorizationUrl: "https://example.test/auth" })
      .mockReturnValueOnce(wait.promise)
      .mockReturnValueOnce(cancel.promise);

    renderSettings();
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "pessoal");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));
    expect(await screen.findByText("Aguardando autenticação no navegador")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Cancelar conexão" }));
    expect(invokeMock).toHaveBeenCalledWith("cancel_openai_codex_connection", {
      flowId: "flow-2",
    });
    cancel.resolve();
    wait.reject({ code: "cancelled", message: "A conexão foi cancelada." });
    expect(await screen.findByRole("dialog", { name: "Adicionar conta" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("mostra a conta quando o cancelamento perde para o commit durável", async () => {
    const connected = account("openai-codex-pessoal");
    const wait = deferred<ProviderAccount>();
    const cancel = deferred<void>();
    const user = userEvent.setup();
    const successSpy = vi.spyOn(toast, "success");
    invokeMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({
        flowId: "flow-commit",
        authorizationUrl: "https://example.test/auth",
      })
      .mockReturnValueOnce(wait.promise)
      .mockReturnValueOnce(cancel.promise)
      .mockResolvedValueOnce([connected]);

    renderSettings();
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "pessoal");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));
    await user.click(await screen.findByRole("button", { name: "Cancelar conexão" }));

    wait.resolve(connected);
    cancel.resolve();

    expect(await screen.findByText(connected.alias)).toBeInTheDocument();
    expect(successSpy).toHaveBeenCalledWith("Conta conectada");
  });

  it("cancela e descarta o fluxo quando fecha antes de abrir o navegador", async () => {
    const begin = deferred<{ flowId: string; authorizationUrl: string }>();
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    invokeMock
      .mockResolvedValueOnce([])
      .mockReturnValueOnce(begin.promise)
      .mockResolvedValueOnce(undefined);

    renderSettings(onOpenChange);
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "empresa");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onOpenChange).not.toHaveBeenCalledWith(false);

    begin.resolve({ flowId: "flow-before-browser", authorizationUrl: "https://example.test/auth" });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("cancel_openai_codex_connection", {
        flowId: "flow-before-browser",
      }),
    );
    expect(openUrlMock).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("wait_openai_codex_connection", expect.anything());
  });

  it("cancela o fluxo ao fechar a modal e mantém Configurações aberta", async () => {
    const user = userEvent.setup();
    const wait = deferred<ProviderAccount>();
    const cancel = deferred<void>();
    const onOpenChange = vi.fn();
    invokeMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ flowId: "flow-3", authorizationUrl: "https://example.test/auth" })
      .mockReturnValueOnce(wait.promise)
      .mockReturnValueOnce(cancel.promise);

    renderSettings(onOpenChange);
    await user.click(await screen.findByRole("button", { name: "Adicionar conta" }));
    await user.type(screen.getByRole("textbox", { name: "Sufixo do alias" }), "empresa");
    await user.click(screen.getByRole("button", { name: "Conectar com ChatGPT" }));
    expect(await screen.findByText("Aguardando autenticação no navegador")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(invokeMock).toHaveBeenCalledWith("cancel_openai_codex_connection", {
      flowId: "flow-3",
    });
    expect(onOpenChange).not.toHaveBeenCalledWith(false);

    cancel.resolve();
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Conectar com ChatGPT" })).not.toBeInTheDocument());
    wait.resolve(account("openai-codex-empresa"));
  });

  it("renderiza dois aliases e exige confirmação antes de desconectar", async () => {
    const first = account("openai-codex-pessoal");
    const second = account("openai-codex-empresa");
    const user = userEvent.setup();
    const successSpy = vi.spyOn(toast, "success");
    let removed = false;
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_provider_removal_plan") return { alias: first.alias, revision: "review-1", items: [] };
      if (command === "disconnect_provider_account") { removed = true; return { replaced: 0, unresolved: [] }; }
      return removed ? [second] : [first, second];
    });

    renderSettings();
    expect(await screen.findByText(first.alias)).toBeInTheDocument();
    expect(screen.getByText(second.alias)).toBeInTheDocument();
    expect(screen.getByTestId(`provider-account-${first.alias}`)).toBeInTheDocument();
    expect(screen.getByTestId(`provider-account-${second.alias}`)).toBeInTheDocument();

    const firstRow = screen.getByTestId(`provider-account-${first.alias}`);
    await user.click(within(firstRow).getByRole("button", { name: `Detalhes de ${first.alias}` }));
    const details = screen.getByRole("dialog", { name: first.alias });
    await user.click(within(details).getByRole("button", { name: "Desconectar" }));
    const confirmation = await screen.findByRole("dialog", { name: "Remover provedor?" });
    expect(confirmation).toHaveTextContent(first.alias);
    expect(invokeMock).not.toHaveBeenCalledWith("disconnect_provider_account", expect.anything());

    const remove = within(confirmation).getByRole("button", { name: "Remover provedor" });
    await waitFor(() => expect(remove).toBeEnabled());
    await user.click(remove);
    await waitFor(() => expect(successSpy).toHaveBeenCalledWith("Provedor removido"));
    expect(invokeMock).toHaveBeenCalledWith("disconnect_provider_account", {
      alias: first.alias,
      revision: "review-1", replacements: [],
    });
  });
});
