import { openUrl } from "@tauri-apps/plugin-opener";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderAccount } from "@/core/provider-accounts";
import SettingsDialog from "./SettingsDialog";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => command === "get_web_search_config"
    ? Promise.resolve({ accountAlias: null })
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
  return onOpenChange;
}

describe("SettingsDialog provider accounts", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    invokeMock.mockReset();
    openUrlMock.mockReset();
    openUrlMock.mockResolvedValue(undefined);
  });

  it("mostra o skeleton ao abrir e depois o estado vazio", async () => {
    const list = deferred<ProviderAccount[]>();
    invokeMock.mockReturnValueOnce(list.promise);

    renderSettings();

    expect(screen.getByRole("status", { name: /carregando contas/i })).toBeInTheDocument();
    list.resolve([]);

    expect(await screen.findByText("Nenhuma conta conectada")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Adicionar conta" })).toBeInTheDocument();
    expect(await screen.findByRole("combobox", { name: "Conta para pesquisa" })).toHaveTextContent("Desligado");
  });

  it("mostra os dados da assinatura e os modelos ao expandir o card", async () => {
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
    expect(within(card).getByText("dev@empresa.com")).toBeInTheDocument();
    expect(within(card).getByText("Enterprise")).toBeInTheDocument();
    expect(within(card).getByText("GPT-5.6 Luna")).toBeInTheDocument();
    expect(within(card).getByText("GPT-5.6 Sol")).toBeInTheDocument();
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
    expect(await screen.findByText("Adicionar conta")).toBeInTheDocument();
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
    expect(onOpenChange).toHaveBeenCalledWith(false);

    begin.resolve({ flowId: "flow-before-browser", authorizationUrl: "https://example.test/auth" });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("cancel_openai_codex_connection", {
        flowId: "flow-before-browser",
      }),
    );
    expect(openUrlMock).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("wait_openai_codex_connection", expect.anything());
  });

  it("cancela o fluxo antes de fechar Configurações", async () => {
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
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
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
    expect(screen.getByRole("button", { name: `Detalhes de ${second.alias}` })).toHaveAttribute("aria-expanded", "false");
    await user.click(within(firstRow).getByRole("button", { name: "Desconectar" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent(first.alias);
    expect(invokeMock).not.toHaveBeenCalledWith("disconnect_provider_account", expect.anything());

    await user.click(screen.getByRole("button", { name: "Desconectar" }));
    await waitFor(() => expect(successSpy).toHaveBeenCalledWith("Conta desconectada"));
    expect(invokeMock).toHaveBeenCalledWith("disconnect_provider_account", {
      alias: first.alias,
    });
  });
});
