import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

describe("App - Onboarding & Design System", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renderiza a tela de onboarding com título e etapas de configuração", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: /bem-vindo ao jarvis/i }),
    ).toBeInTheDocument();

    expect(
      screen.getByText(/seu ambiente autônomo de engenharia de software/i),
    ).toBeInTheDocument();

    expect(screen.getByText("Provedores de Inteligência")).toBeInTheDocument();
    expect(screen.getByText("Workspace & Repositório")).toBeInTheDocument();
    expect(
      screen.getByText("Permissões & Ferramentas Locais"),
    ).toBeInTheDocument();

    expect(
      screen.getByRole("button", { name: /começar configuração/i }),
    ).toBeInTheDocument();
  });

  it("exibe badge e rodapé indicando Roboto e tema One Dark", () => {
    render(<App />);

    expect(screen.getByText("One Dark Theme")).toBeInTheDocument();
    expect(screen.getByText("One Dark")).toBeInTheDocument();
    expect(screen.getByText("Roboto")).toBeInTheDocument();
  });
  it("dispara notificação sonner 'Em breve' ao clicar no botão de começar configuração", async () => {
    const user = userEvent.setup();
    const toastInfoSpy = vi.spyOn(toast, "info");

    render(<App />);

    const startButton = screen.getByRole("button", {
      name: /começar configuração/i,
    });

    await user.click(startButton);

    expect(toastInfoSpy).toHaveBeenCalledTimes(1);
    expect(toastInfoSpy).toHaveBeenCalledWith(
      "Em breve",
      expect.objectContaining({
        description: expect.stringMatching(/assistente de configuração guiada/i),
      }),
    );

    // O botão atualiza seu estado visual indicando o início
    expect(
      screen.getByRole("button", { name: /configurando\.\.\./i }),
    ).toBeInTheDocument();
  });

  it("dispara toast informativo ao clicar no botão de docs de referência", async () => {
    const user = userEvent.setup();
    const toastSpy = vi.spyOn(toast, "message");

    render(<App />);

    const docsButton = screen.getByRole("button", {
      name: /docs de referência/i,
    });

    await user.click(docsButton);

    expect(toastSpy).toHaveBeenCalled();
    expect(docsButton).toBeEnabled();
  });
});
