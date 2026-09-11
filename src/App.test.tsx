import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import {
  emptyLibrary,
  populatedLibrary,
} from "@/test/library-fixtures";
import { emptyChat } from "@/test/chat-fixtures";
import { coreFixture } from "@/test/core-fixtures";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("@/components/ui/sonner", () => ({
  Toaster: ({ position }: { position?: string }) => (
    <div data-testid="toaster" data-position={position} />
  ),
}));

type AppConfig = {
  onboardingCompleted: boolean;
};

const invokeMock = vi.mocked(invoke);

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });

  return { promise, resolve, reject };
}
const connectedAccount = { alias: "openai-codex-test", providerKind: "openai-codex", enabled: true, createdAt: 1, email: null, accountType: "personal", modelsAvailable: true, models: [{ id: "test", name: "Test", reasoningLevels: [], defaultReasoningLevel: null }] };
const optionalTools = {
  platform: "macos",
  platformLabel: "macOS",
  tools: [
    { id: "git", name: "Git", description: "Versionamento", installed: true, version: "git version 2.51.0", automaticInstall: true, installWith: "Homebrew", helpUrl: "https://git-scm.com/download/mac" },
    { id: "gh", name: "GitHub CLI", description: "Pull requests", installed: true, version: "gh version 2.80.0", automaticInstall: true, installWith: "Homebrew", helpUrl: "https://cli.github.com/" },
  ],
};
async function reachFinish(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Avançar" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  return screen.getByRole("button", { name: "Começar" });
}
function titleBar() {
  return screen.getAllByRole("banner")[0];
}

describe("App bootstrap and onboarding", () => {
  beforeAll(async () => {
    // Exercise the real Home while keeping module transformation outside UI wait deadlines.
    await import("@/components/layout/Home");
  });
  beforeEach(() => {
    vi.restoreAllMocks();
    invokeMock.mockReset();
    invokeMock.mockImplementation((command, args) => {
      if (command === "get_core_status" || command === "check_core_updates") return Promise.resolve(coreFixture());
      if (command === "get_optional_tools_status") return Promise.resolve(optionalTools);
      if (command === "list_provider_accounts") return Promise.resolve([connectedAccount]);
      if (command === "get_provider_usage") return Promise.resolve({ alias: (args as { alias: string }).alias, fetchedAt: Date.now(), email: null, plan: "plus", windows: [], error: null, resetCredits: null });
      if (command === "list_skills") return Promise.resolve({ includeAgents: false, directory: "/home/.jarvis/skills", skills: [], warnings: [] });
      if (command === "get_web_search_config" || command === "get_vision_config") return Promise.resolve({ accountAlias: null, model: null, inheritChat: true });
      if (command === "get_agent_activity") return Promise.resolve([]);
      if (command === "get_library_snapshot")
        return Promise.resolve(emptyLibrary());
      return Promise.reject(new Error(`Unexpected Tauri command: ${command}`));
    });
  });

  it("suppresses the native menu including portals and permits only scoped project menus", async () => {
    invokeMock.mockImplementation((command) => {
      if (command === "get_core_status" || command === "check_core_updates") return Promise.resolve(coreFixture());
      if (command === "get_app_config")
        return Promise.resolve({ onboardingCompleted: true });
      if (command === "get_library_snapshot")
        return Promise.resolve(populatedLibrary());
      if (command === "get_chat")
        return Promise.resolve(emptyChat());
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    const { unmount } = render(<App />);
    const sidebar = await screen.findByRole("complementary", { name: "Workspace" });
    const project = await within(sidebar).findByRole("button", {
      name: "Jarvis",
    });
    const backgroundEvent = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
    });
    fireEvent(document.body, backgroundEvent);
    expect(backgroundEvent.defaultPrevented).toBe(true);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    fireEvent.contextMenu(project);
    await user.click(await screen.findByRole("menuitem", { name: "Editar" }));
    const name = await screen.findByLabelText("Nome do projeto");
    const portalEvent = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
    });
    fireEvent(name, portalEvent);
    expect(portalEvent.defaultPrevented).toBe(true);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    unmount();
    const afterUnmount = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
    });
    fireEvent(document.body, afterUnmount);
    expect(afterUnmount.defaultPrevented).toBe(false);
  });

  it("mantém loading visível e não mostra onboarding antes da leitura persistida", async () => {
    const config = deferred<AppConfig>();
    const fallback = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args, options) =>
      command === "get_app_config"
        ? config.promise
        : fallback(command, args, options),
    );

    render(<App />);

    expect(screen.getByRole("status", { name: "Iniciando o Jarvis" })).toBeVisible();
    expect(screen.queryByText("Carregando configuração…")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).not.toBeInTheDocument();
    expect(within(titleBar()).getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "/logo_horizontal.png");

    config.resolve({ onboardingCompleted: false });
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: /bem-vindo ao jarvis/i }),
      ).toBeInTheDocument(),
    );
  });

  it("posiciona os toasts no topo central da janela", () => {
    const fallback = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args, options) =>
      command === "get_app_config"
        ? Promise.resolve({ onboardingCompleted: false })
        : fallback(command, args, options),
    );

    render(<App />);

    expect(screen.getByTestId("toaster")).toHaveAttribute("data-position", "top-center");
  });

  it("leva configuração incompleta ao onboarding com o CTA Avançar", async () => {
    const fallback = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args, options) =>
      command === "get_app_config"
        ? Promise.resolve({ onboardingCompleted: false })
        : fallback(command, args, options),
    );

    render(<App />);

    expect(
      await screen.findByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).toBeInTheDocument();
    expect(within(titleBar()).getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "/logo_horizontal.png");
    expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled();
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
  });

  it("leva configuração concluída diretamente ao Home", async () => {
    const fallback = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args, options) =>
      command === "get_app_config"
        ? Promise.resolve({ onboardingCompleted: true })
        : fallback(command, args, options),
    );

    render(<App />);

    expect(
      await screen.findByTestId("home-shell", {}, { timeout: 5_000 }),
    ).toBeInTheDocument();
    expect(within(titleBar()).getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "/logo_horizontal.png");
    expect(
      screen.queryByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.filter(([command]) => command === "check_core_updates")).toHaveLength(1);
    expect(invokeMock.mock.calls.filter(([command]) => command === "get_core_status")).toHaveLength(0);
    expect(invokeMock.mock.calls.filter(([command]) => command === "list_provider_accounts")).toHaveLength(1);
    expect(invokeMock.mock.calls.filter(([command]) => command === "get_provider_usage")).toHaveLength(1);
    expect(invokeMock.mock.calls.filter(([command]) => command === "get_library_snapshot")).toHaveLength(1);
  });

  it("bloqueia em erro de carga e permite tentar novamente", async () => {
    const retryConfig = deferred<AppConfig>();
    const fallback = invokeMock.getMockImplementation()!;
    let configReads = 0;
    invokeMock.mockImplementation((command, args, options) => {
      if (command === "get_app_config") {
        configReads += 1;
        return configReads === 1 ? Promise.reject(new Error("database unavailable")) : retryConfig.promise;
      }
      return fallback(command, args, options);
    });

    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(within(titleBar()).getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "/logo_horizontal.png");
    expect(
      screen.queryByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    retryConfig.resolve({ onboardingCompleted: false });
    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).toBeInTheDocument();
    expect(invokeMock.mock.calls.filter(([command]) => command === "get_app_config")).toHaveLength(2);
  });

  it("mantém Finalizar pendente e só mostra Home após conclusão confirmada", async () => {
    const completion = deferred<AppConfig>();
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args) => command === "complete_onboarding" ? completion.promise : command === "get_app_config" ? Promise.resolve({ onboardingCompleted: false }) : base(command, args));

    const user = userEvent.setup();
    render(<App />);

    const finishButton = await reachFinish(user);
    await user.click(finishButton);

    expect(screen.getByRole("button", { name: /preparando/i })).toBeDisabled();
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("complete_onboarding", { workspaceName: "" });

    completion.resolve({ onboardingCompleted: true });
    expect(
      await screen.findByTestId("home-shell", {}, { timeout: 5_000 }),
    ).toBeInTheDocument();
    expect(within(titleBar()).getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "/logo_horizontal.png");
  });

  it("permanece no onboarding, reabilita Finalizar e mostra erro quando conclusão falha", async () => {
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args) => command === "complete_onboarding" ? Promise.reject(new Error("write failed")) : command === "get_app_config" ? Promise.resolve({ onboardingCompleted: false }) : base(command, args));

    const toastErrorSpy = vi.spyOn(toast, "error");
    const user = userEvent.setup();
    render(<App />);

    await user.click(await reachFinish(user));

    await waitFor(() =>
      expect(toastErrorSpy).toHaveBeenCalledWith(
        "Não foi possível concluir o onboarding",
        expect.objectContaining({ description: "Tente novamente." }),
      ),
    );
    expect(screen.getByRole("button", { name: "Começar" })).toBeEnabled();
    expect(within(titleBar()).getByRole("img", { name: "Jarvis" })).toHaveAttribute("src", "/logo_horizontal.png");
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
  });

  it("não considera conclusão confirmada quando o comando retorna false", async () => {
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args) => command === "complete_onboarding" || command === "get_app_config" ? Promise.resolve({ onboardingCompleted: false }) : base(command, args));

    const user = userEvent.setup();
    render(<App />);
    await user.click(await reachFinish(user));

    expect(
      await screen.findByRole("button", { name: "Começar" }),
    ).toBeEnabled();
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
  });

  it("apresenta os recursos disponíveis sem detalhes internos de implementação", async () => {
    const fallback = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((command, args, options) =>
      command === "get_app_config"
        ? Promise.resolve({ onboardingCompleted: false })
        : fallback(command, args, options),
    );

    render(<App />);

    expect(
      await screen.findByText("Seus modelos"),
    ).toBeInTheDocument();
    expect(screen.getByText("Tudo organizado")).toBeInTheDocument();
    expect(
      screen.getByText("Do plano ao código"),
    ).toBeInTheDocument();
    expect(screen.getByText("Conecte provedores e escolha o modelo ideal para cada agente.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled();
    expect(screen.queryByText(/docs\/metis/)).not.toBeInTheDocument();
  });
});
