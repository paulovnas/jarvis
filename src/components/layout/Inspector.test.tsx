import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { emptyLibrary, populatedLibrary } from "@/test/library-fixtures";
import { Inspector } from "./Inspector";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";

describe("Inspector", () => {
  it("shows real execution metadata and only confirmed file writes", () => {
    const turn = savedTurn();
    turn.steps[0].tools = [
      { ...turn.steps[0].tools[0], name: "edit", args: { path: "src/main.ts" } },
      { ...turn.steps[0].tools[0], id: "failed", name: "write", args: { path: "missing.txt" }, status: "error" },
    ];
    render(<Inspector library={populatedLibrary()} chat={{ ...emptyChat(), turns: [turn] }} />);
    expect(screen.getByText("src/main.ts")).toBeInTheDocument();
    expect(screen.queryByText("missing.txt")).not.toBeInTheDocument();
    expect(screen.getByText(/150 entrada · 40 saída/)).toBeInTheDocument();
    expect(screen.getByText("Build · Manual")).toBeInTheDocument();
  });
  it("shows only the selected context and no fabricated execution", () => {
    render(<Inspector library={populatedLibrary()} />);
    expect(screen.getByText("Pessoal")).toBeInTheDocument();
    expect(screen.getByText("/projects/jarvis")).toBeInTheDocument();
    expect(screen.getByText("Primeira conversa")).toBeInTheDocument();
    expect(screen.queryByText("Conversa do trabalho")).not.toBeInTheDocument();
    expect(
      screen.getByText("Nenhuma alteração registrada."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });
  it("clears project and conversation details with the selection", () => {
    const { rerender } = render(<Inspector library={populatedLibrary()} />);
    rerender(<Inspector library={emptyLibrary()} />);
    expect(
      screen.getByText("Nenhum workspace selecionado."),
    ).toBeInTheDocument();
    expect(screen.queryByText("/projects/jarvis")).not.toBeInTheDocument();
  });
});
