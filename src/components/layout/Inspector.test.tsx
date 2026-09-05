import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { emptyLibrary, populatedLibrary } from "@/test/library-fixtures";
import { Inspector } from "./Inspector";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";

describe("Inspector", () => {
  it("uses the compacted backend context instead of stale pre-compaction usage", async () => {
    render(<Inspector library={populatedLibrary()} chat={{ ...emptyChat(), turns: [savedTurn()], context: { tokens: 40, limit: 1000, estimated: true, compacting: false, compactions: 1 } }} />);
    const footer = screen.getByRole("contentinfo", { name: "Contexto da conversa" });
    expect(within(footer).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "4");
    expect(within(footer).queryByText(/realizada/)).not.toBeInTheDocument();
    expect(within(footer).queryByText("150")).not.toBeInTheDocument();
  });
  it("groups confirmed file edits and keeps context in the footer across tabs", async () => {
    const user = userEvent.setup();
    const turn = savedTurn();
    turn.contextWindow = 1000;
    turn.steps[0].tools = [
      { ...turn.steps[0].tools[0], name: "edit", args: { path: "src/main.ts", oldText: "old", newText: "new\nline" }, output: "Alteração salva." },
      { ...turn.steps[0].tools[0], id: "failed", name: "write", args: { path: "missing.txt" }, status: "error", output: "" },
    ];
    render(<Inspector library={populatedLibrary()} chat={{ ...emptyChat(), turns: [turn], fileChanges: [{ path: "src/main.ts", additions: 2, deletions: 1, base: "conversation" }] }} />);
    expect(screen.getByRole("tab", { name: "Atividades" })).toHaveAttribute("aria-selected", "true");
    const file = screen.getByRole("button", { name: "Alterações em src/main.ts" });
    expect(file).toHaveTextContent("+2");
    expect(file).toHaveTextContent("−1");
    expect(screen.queryByText("missing.txt")).not.toBeInTheDocument();
    expect(screen.queryByText("Alteração salva.")).not.toBeInTheDocument();
    const footer = screen.getByRole("contentinfo", { name: "Contexto da conversa" });
    expect(Number(within(footer).getByRole("progressbar").getAttribute("aria-valuenow"))).toBeCloseTo(19.4);
    expect(within(footer).queryByText(/Estimativa:/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Detalhes" }));
    expect(await screen.findByText("Build · Manual")).toBeInTheDocument();
    expect(screen.getByText("/projects/jarvis")).toBeInTheDocument();
    expect(footer).toBeInTheDocument();
  });

  it("shows empty activity sections and an unknown context without fabricating data", async () => {
    const user = userEvent.setup();
    render(<Inspector library={populatedLibrary()} />);
    expect(screen.getByText("Nenhuma alteração registrada.")).toBeInTheDocument();
    expect(screen.getByText("Nenhum plano.")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Detalhes" }));
    expect(await screen.findByText("Pessoal")).toBeInTheDocument();
    expect(screen.getByText("/projects/jarvis")).toBeInTheDocument();
    expect(screen.getByText("Primeira conversa")).toBeInTheDocument();
    expect(screen.queryByText("Conversa do trabalho")).not.toBeInTheDocument();
  });

  it("clears stale files and context when another conversation is selected", async () => {
    const user = userEvent.setup();
    const turn = savedTurn();
    turn.contextWindow = 1000;
    turn.steps[0].tools[0].name = "write";
    const chat = { ...emptyChat(), turns: [turn] };
    const { rerender } = render(<Inspector library={populatedLibrary()} chat={chat} />);
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
    rerender(<Inspector library={emptyLibrary()} chat={chat} />);
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Alterações em/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Detalhes" }));
    await waitFor(() => expect(screen.getByText("Nenhum workspace selecionado.")).toBeInTheDocument());
  });
});
