import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
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
function titleBar() {
  return screen.getAllByRole("banner")[0];
}


describe("App bootstrap and onboarding", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    invokeMock.mockReset();
  });

  it("mantém loading visível e não mostra onboarding antes da leitura persistida", async () => {
    const config = deferred<AppConfig>();
    invokeMock.mockReturnValueOnce(config.promise);

    render(<App />);

    expect(screen.getByRole("status")).toHaveTextContent(
      "Carregando configuração…",
    );
    expect(
      screen.queryByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).not.toBeInTheDocument();
    expect(titleBar()).toHaveTextContent("Iniciando");

    config.resolve({ onboardingCompleted: false });
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: /bem-vindo ao jarvis/i }),
      ).toBeInTheDocument(),
    );
  });

  it("leva configuração incompleta ao onboarding com o CTA Finalizar", async () => {
    invokeMock.mockResolvedValueOnce({ onboardingCompleted: false });

    render(<App />);

    expect(
      await screen.findByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).toBeInTheDocument();
    expect(titleBar()).toHaveTextContent("Onboarding");
    expect(
      screen.getByRole("button", { name: "Finalizar" }),
    ).toBeEnabled();
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
  });

  it("leva configuração concluída diretamente ao Home", async () => {
    invokeMock.mockResolvedValueOnce({ onboardingCompleted: true });

    render(<App />);

    expect(await screen.findByTestId("home-shell")).toBeInTheDocument();
    expect(titleBar()).toHaveTextContent("Início");
    expect(
      screen.queryByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).not.toBeInTheDocument();
  });

  it("bloqueia em erro de carga e permite tentar novamente", async () => {
    const retryConfig = deferred<AppConfig>();
    invokeMock
      .mockRejectedValueOnce(new Error("database unavailable"))
      .mockReturnValueOnce(retryConfig.promise);

    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(titleBar()).toHaveTextContent("Iniciando");
    expect(
      screen.queryByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Tentar novamente" }),
    );
    retryConfig.resolve({ onboardingCompleted: false });
    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenNthCalledWith(2, "get_app_config");
  });

  it("mantém Finalizar pendente e só mostra Home após conclusão confirmada", async () => {
    const completion = deferred<AppConfig>();
    invokeMock
      .mockResolvedValueOnce({ onboardingCompleted: false })
      .mockReturnValueOnce(completion.promise);

    const user = userEvent.setup();
    render(<App />);

    const finishButton = await screen.findByRole("button", {
      name: "Finalizar",
    });
    await user.click(finishButton);

    expect(
      screen.getByRole("button", { name: /finalizando/i }),
    ).toBeDisabled();
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenNthCalledWith(2, "complete_onboarding");

    completion.resolve({ onboardingCompleted: true });
    expect(await screen.findByTestId("home-shell")).toBeInTheDocument();
    expect(titleBar()).toHaveTextContent("Início");
  });

  it("permanece no onboarding, reabilita Finalizar e mostra erro quando conclusão falha", async () => {
    invokeMock
      .mockResolvedValueOnce({ onboardingCompleted: false })
      .mockRejectedValueOnce(new Error("write failed"));

    const toastErrorSpy = vi.spyOn(toast, "error");
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Finalizar" }),
    );

    await waitFor(() =>
      expect(toastErrorSpy).toHaveBeenCalledWith(
        "Não foi possível concluir o onboarding",
        expect.objectContaining({ description: "Tente novamente." }),
      ),
    );
    expect(
      screen.getByRole("button", { name: "Finalizar" }),
    ).toBeEnabled();
    expect(titleBar()).toHaveTextContent("Onboarding");
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
  });

  it("não considera conclusão confirmada quando o comando retorna false", async () => {
    invokeMock
      .mockResolvedValueOnce({ onboardingCompleted: false })
      .mockResolvedValueOnce({ onboardingCompleted: false });

    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Finalizar" }),
    );

    expect(
      await screen.findByRole("button", { name: "Finalizar" }),
    ).toBeEnabled();
    expect(screen.queryByTestId("home-shell")).not.toBeInTheDocument();
  });

  it("preserva o conteúdo visual do onboarding e o toast dos docs", async () => {
    invokeMock.mockResolvedValueOnce({ onboardingCompleted: false });

    render(<App />);

    expect(
      await screen.findByText("Provedores de Inteligência"),
    ).toBeInTheDocument();
    expect(screen.getByText("Workspace & Repositório")).toBeInTheDocument();
    expect(
      screen.getByText("Permissões & Ferramentas Locais"),
    ).toBeInTheDocument();
    expect(screen.getByText("One Dark Theme")).toBeInTheDocument();
    expect(screen.getByText("One Dark")).toBeInTheDocument();
    expect(screen.getByText("Roboto")).toBeInTheDocument();

    const toastSpy = vi.spyOn(toast, "message");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /docs de referência/i }));

    expect(toastSpy).toHaveBeenCalledWith(
      "Base de Conhecimento",
      expect.objectContaining({
        description: expect.stringMatching(/docs\/metis/i),
      }),
    );
  });
});
