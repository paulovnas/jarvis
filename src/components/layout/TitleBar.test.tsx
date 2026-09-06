import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TitleBar } from "./TitleBar";

describe("TitleBar Component", () => {
  it("shows window controls in macOS order before the branding without a page label", () => {
    render(<TitleBar />);
    const buttons = screen.getAllByRole("button");
    expect(buttons.map(button => button.getAttribute("aria-label"))).toEqual(["Fechar janela", "Minimizar janela", "Maximizar janela"]);
    expect(screen.getByRole("banner")).toHaveTextContent(/^Jarvis$/);
    expect(buttons[2].compareDocumentPosition(screen.getByText("Jarvis")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("does not maximize when double clicking the minimize control", async () => {
    const user = userEvent.setup(); render(<TitleBar />);
    await user.dblClick(screen.getByRole("button", { name: "Minimizar janela" }));
    expect(screen.getByRole("button", { name: "Maximizar janela" })).toBeInTheDocument();
  });

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
