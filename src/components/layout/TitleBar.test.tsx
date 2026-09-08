import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TitleBar } from "./TitleBar";

describe("TitleBar Component", () => {
  it("shows branding first and native caption controls last on non-macOS", () => {
    render(<TitleBar />);
    const buttons = screen.getAllByRole("button");
    // Windows/Linux caption order: minimize, maximize/restore, close (right side).
    expect(buttons.map(button => button.getAttribute("aria-label"))).toEqual(["Minimizar janela", "Maximizar janela", "Fechar janela"]);
    const logo = screen.getByRole("img", { name: "Jarvis" });
    expect(logo).toHaveAttribute("src", "/logo_horizontal.png");
    expect(logo).toHaveAttribute("draggable", "false");
    expect(screen.queryByText("Jarvis")).not.toBeInTheDocument();
    // The wordmark precedes the controls in the DOM (branding left, controls right).
    expect(logo.compareDocumentPosition(buttons[0]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
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
