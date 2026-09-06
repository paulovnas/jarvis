import { openUrl } from "@tauri-apps/plugin-opener";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderAccount } from "@/core/provider-accounts";
import SettingsDialog from "./SettingsDialog";
import { coreFixture } from "@/test/core-fixtures";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => command === "get_web_search_config" || command === "get_vision_config"
    ? Promise.resolve({ accountAlias: null })
    : command === "list_mcp_servers" ? Promise.resolve([])
    : command === "list_skills" ? Promise.resolve({ includeAgents: false, directory: "/home/.jarvis/skills", skills: [], warnings: [] })
    : command === "get_core_status" || command === "check_core_updates" ? Promise.resolve(coreFixture())
    : args === undefined ? invokeMock(command) : invokeMock(command, args),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
}));

const openUrlMock = vi.mocked(openUrl);

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
  it("identifica a aba ativa e mantém a navegação disponível ao trocar o conteúdo", async () => {
    const user = userEvent.setup();
    invokeMock.mockResolvedValue([]);
    renderSettings();
    const navigation = screen.getByRole("tablist", { name: "Configurações" });
    const providers = within(navigation).getByRole("tab", { name: /Provedores/ });
    expect(providers).toHaveAttribute("aria-selected", "true");
    expect(providers).toHaveAttribute("data-active");
    const skills = within(navigation).getByRole("tab", { name: /Skills/ });
    await user.click(skills);
    expect(skills).toHaveAttribute("data-active");
    expect(skills).toHaveAttribute("aria-selected", "true");
    expect(providers).not.toHaveAttribute("data-active");
    expect(navigation).toBeVisible();
    expect(screen.getAllByRole("tab")).toHaveLength(6);
    await user.click(screen.getByRole("tab", { name: "Geral" }));
    expect(screen.queryByRole("region", { name: "Core" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Ferramentas" }));
    expect(await screen.findByRole("region", { name: "Core" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "Ferramentas" })).toHaveAttribute("aria-selected", "true");
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
    vi.restoreAllMocks();
    invokeMock.mockReset();
    openUrlMock.mockReset();
    openUrlMock.mockResolvedValue(undefined);
  });

  it("ordena as abas, apresenta Core em Ferramentas e mantém Skills disponível", async () => {
    invokeMock.mockResolvedValueOnce([account("openai-codex-pessoal")]);
    const user = userEvent.setup();
    render(<SettingsDialog open onOpenChange={vi.fn()} />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map(tab => tab.textContent?.replace(/\\d/g, ""))).toEqual(["Geral", "Ferramentas", "Agentes", "Provedores", "Skills", "MCPs"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(within(screen.getByRole("tabpanel", { name: "Geral" })).queryByRole("heading", { name: "Core" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Ferramentas" }));
    expect(await within(screen.getByRole("tabpanel", { name: "Ferramentas" })).findByRole("heading", { name: "Core" })).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Skills/ }));
    expect(await screen.findByRole("button", { name: "Marketplace" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("tab", { name: /Skills/ })).toHaveTextContent("0"));
    await waitFor(() => expect(screen.getByRole("tab", { name: /MCPs/ })).toHaveTextContent("0"));
    expect(screen.getByRole("tab", { name: /Provedores/ })).toHaveTextContent("1");
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

    expect(await screen.findByText("Nenhuma conta conectada")).toBeInTheDocument();
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
    expect(within(card).getByText("OpenAI Codex · 2 modelos")).toBeInTheDocument();
    expect(within(card).queryByText("dev@empresa.com")).not.toBeInTheDocument();
    expect(within(card).queryByText("GPT-5.6 Luna")).not.toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: `Detalhes de ${connected.alias}` }));
    const details = screen.getByRole("dialog", { name: connected.alias });
    expect(within(details).getByText("dev@empresa.com")).toBeInTheDocument();
    expect(within(details).getByText("Enterprise")).toBeInTheDocument();
    expect(within(details).getByText("GPT-5.6 Luna")).toBeInTheDocument();
    expect(within(details).getByText("GPT-5.6 Sol")).toBeInTheDocument();
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
    invokeMock.mockResolvedValueOnce([first, second]).mockResolvedValueOnce([]);

    renderSettings();
    expect(await screen.findByText(first.alias)).toBeInTheDocument();
    expect(screen.getByText(second.alias)).toBeInTheDocument();
    expect(screen.getByTestId(`provider-account-${first.alias}`)).toBeInTheDocument();
    expect(screen.getByTestId(`provider-account-${second.alias}`)).toBeInTheDocument();

    const firstRow = screen.getByTestId(`provider-account-${first.alias}`);
    await user.click(within(firstRow).getByRole("button", { name: `Detalhes de ${first.alias}` }));
    const details = screen.getByRole("dialog", { name: first.alias });
    await user.click(within(details).getByRole("button", { name: "Desconectar" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent(first.alias);
    expect(invokeMock).not.toHaveBeenCalledWith("disconnect_provider_account", expect.anything());

    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Desconectar" }));
    await waitFor(() => expect(successSpy).toHaveBeenCalledWith("Conta desconectada"));
    expect(invokeMock).toHaveBeenCalledWith("disconnect_provider_account", {
      alias: first.alias,
    });
  });
});
