import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TitleBar } from "./TitleBar";

describe("TitleBar Component", () => {
  it("renderiza branding do Jarvis e indicador de contexto", () => {
    render(<TitleBar context="Onboarding" />);

    expect(screen.getByText("Jarvis")).toBeInTheDocument();
    expect(screen.getByText("Onboarding")).toBeInTheDocument();
  });

  it.each(["Iniciando", "Onboarding", "Início"] as const)(
    "renderiza o contexto %s",
    (context) => {
      render(<TitleBar context={context} />);

      expect(screen.getByRole("banner")).toHaveTextContent(context);
    },
  );

  it("renderiza os botões de controle de janela com acessibilidade e cursor-pointer", () => {
    render(<TitleBar />);

    const minimizeBtn = screen.getByRole("button", { name: /minimizar janela/i });
    const maximizeBtn = screen.getByRole("button", {
      name: /maximizar janela|restaurar janela/i,
    });
    const closeBtn = screen.getByRole("button", { name: /fechar janela/i });

    expect(minimizeBtn).toBeInTheDocument();
    expect(maximizeBtn).toBeInTheDocument();
    expect(closeBtn).toBeInTheDocument();

    expect(minimizeBtn).toHaveClass("cursor-pointer");
    expect(maximizeBtn).toHaveClass("cursor-pointer");
    expect(closeBtn).toHaveClass("cursor-pointer");
  });

  it("permite clicar nos controles de janela sem lançar exceções fora do Tauri", async () => {
    const user = userEvent.setup();
    render(<TitleBar />);

    const minimizeBtn = screen.getByRole("button", { name: /minimizar janela/i });
    const maximizeBtn = screen.getByRole("button", {
      name: /maximizar janela|restaurar janela/i,
    });
    const closeBtn = screen.getByRole("button", { name: /fechar janela/i });

    await user.click(minimizeBtn);
    await user.click(maximizeBtn);
    await user.click(closeBtn);

    expect(minimizeBtn).toBeEnabled();
    expect(closeBtn).toBeEnabled();
  });

  it("alterna estado de maximizado ao dar dois cliques na barra", async () => {
    const user = userEvent.setup();
    render(<TitleBar />);

    const header = screen.getByRole("banner");
    await user.dblClick(header);

    // O botão deve mudar o label para restaurar janela após maximizar
    expect(
      screen.getByRole("button", { name: /restaurar janela/i }),
    ).toBeInTheDocument();
  });
});
