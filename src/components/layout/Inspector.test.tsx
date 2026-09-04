import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Inspector } from "./Inspector";

describe("Inspector component", () => {
  it("renderiza todas as seções e o progresso de contexto", () => {
    render(<Inspector />);

    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeInTheDocument();
    expect(screen.getByText("Inspector")).toBeInTheDocument();
    expect(screen.getByText("Contexto da sessão")).toBeInTheDocument();

    expect(screen.getByText("Arquivos alterados")).toBeInTheDocument();
    expect(screen.getByText("Plano")).toBeInTheDocument();
    expect(screen.getByText("Subagentes")).toBeInTheDocument();
    expect(screen.getByText("Contexto")).toBeInTheDocument();
    expect(screen.getByText("38%")).toBeInTheDocument();
  });

  it("permite colapsar e expandir seções individualmente", async () => {
    const user = userEvent.setup();
    render(<Inspector />);

    // Arquivos alterados inicialmente visíveis
    expect(screen.getByText("src/components/chat/ChatArea.tsx")).toBeInTheDocument();

    const filesToggle = screen.getByRole("button", { name: /Arquivos alterados/i });
    expect(filesToggle).toHaveAttribute("aria-expanded", "true");

    // Clica para colapsar
    await user.click(filesToggle);
    expect(filesToggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("src/components/chat/ChatArea.tsx")).not.toBeInTheDocument();

    // Clica novamente para reabrir
    await user.click(filesToggle);
    expect(filesToggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("src/components/chat/ChatArea.tsx")).toBeInTheDocument();
  });

  it("permite recolher e expandir todos os cards pelo botão global do header", async () => {
    const user = userEvent.setup();
    render(<Inspector />);

    const toggleAllBtn = screen.getByRole("button", { name: /Recolher todos os cards/i });
    expect(toggleAllBtn).toBeInTheDocument();

    // Recolhe todos
    await user.click(toggleAllBtn);

    expect(screen.queryByText("src/components/chat/ChatArea.tsx")).not.toBeInTheDocument();
    expect(screen.queryByText(/Persistência SQLite/i)).not.toBeInTheDocument();
    expect(screen.queryByText("Full Construtor")).not.toBeInTheDocument();

    // Botão agora oferece Expandir
    expect(
      screen.getByRole("button", { name: /Expandir todos os cards/i })
    ).toBeInTheDocument();

    // Expande todos de volta
    await user.click(screen.getByRole("button", { name: /Expandir todos os cards/i }));
    expect(screen.getByText("src/components/chat/ChatArea.tsx")).toBeInTheDocument();
    expect(screen.getByText(/Persistência SQLite/i)).toBeInTheDocument();
  });
});
