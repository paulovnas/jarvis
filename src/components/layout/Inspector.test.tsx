import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { emptyLibrary, populatedLibrary } from "@/test/library-fixtures";
import { Inspector } from "./Inspector";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockResolvedValue([]); });

describe("Inspector", () => {
  it("shows native tasks for direct flows and keeps Beads plans for larger flows", async () => {
    const chat = emptyChat();
    const turn = savedTurn();
    turn.options.workflow = "standard";
    turn.tasks = [
      { id: "inspect", title: "Entender a solicitação", status: "completed" },
      { id: "build", title: "Implementar a mudança", status: "in_progress" },
    ];
    const directChat = { ...chat, turns: [turn], activeTurnId: turn.id };
    const view = render(<Inspector library={populatedLibrary()} chat={directChat} />);
    expect(screen.getByRole("button", { name: /Tarefas.*2/ })).toBeVisible();
    expect(screen.getByText("Implementar a mudança")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Plano" })).not.toBeInTheDocument();

    turn.options.workflow = "designer";
    view.rerender(<Inspector library={populatedLibrary()} chat={{ ...directChat, turns: [{ ...turn }] }} />);
    expect(screen.getByRole("button", { name: /Tarefas.*2/ })).toBeVisible();

    turn.options.workflow = "custom";
    turn.options.customAgentId = "a".repeat(32);
    view.rerender(<Inspector library={populatedLibrary()} chat={{ ...directChat, turns: [{ ...turn }] }} />);
    expect(screen.getByRole("button", { name: /Tarefas.*2/ })).toBeVisible();

    turn.options.workflow = "planned";
    delete turn.options.customAgentId;
    view.rerender(<Inspector library={populatedLibrary()} chat={{ ...directChat, turns: [{ ...turn }] }} />);
    expect(screen.getByRole("button", { name: "Plano" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Tarefas/ })).not.toBeInTheDocument();
  });

  it("shows subagents and manual validation only in the selected Planned and Complete workflows", async () => {
    const chat = emptyChat(); const library = populatedLibrary();
    const workflow = { data: { conversationId: chat.conversationId, revision: 1, flow: "planned" as const, agents: [], validation: null }, error: null, loading: false, retry: vi.fn() };
    const view = render(<Inspector library={library} chat={chat} workflow={workflow} />);
    expect(screen.getByRole("button", { name: "Validação" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
    view.rerender(<Inspector library={library} chat={chat} workflow={{ ...workflow, data: { ...workflow.data, flow: "complete" } }} />);
    expect(screen.getByRole("button", { name: "Validação" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
    for (const flow of ["standard", "designer"] as const) {
      view.rerender(<Inspector library={library} chat={chat} workflow={{ ...workflow, data: { ...workflow.data, flow } }} />);
      expect(screen.queryByRole("button", { name: "Validação" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Subagentes" })).not.toBeInTheDocument();
    }
    view.rerender(<Inspector library={library} chat={chat} workflow={{ ...workflow, data: { ...workflow.data, flow: "custom" } }} />);
    expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Validação" })).not.toBeInTheDocument();
    const turn = savedTurn();
    turn.options.workflow = "custom";
    turn.options.customAgentId = "a".repeat(32);
    view.rerender(<Inspector library={library} chat={{ ...chat, turns: [turn] }} workflow={{ ...workflow, data: { ...workflow.data, flow: "custom" } }} />);
    expect(screen.getByRole("button", { name: /Tarefas/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Subagentes" })).not.toBeInTheDocument();
    view.rerender(<Inspector library={library} chat={chat} workflow={{ ...workflow, data: { ...workflow.data, conversationId: "other" } }} />);
    expect(screen.queryByRole("button", { name: "Validação" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Subagentes" })).not.toBeInTheDocument();
  });
  it("uses the compacted backend context instead of stale pre-compaction usage", async () => {
    render(<Inspector library={populatedLibrary()} chat={{ ...emptyChat(), turns: [savedTurn()], context: { tokens: 40, limit: 1000, estimated: true, compacting: false, compactions: 1 } }} />);
    const footer = screen.getByRole("contentinfo", { name: "Contexto da conversa" });
    expect(within(footer).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "4");
    expect(within(footer).queryByText(/realizada/)).not.toBeInTheDocument();
    expect(within(footer).queryByText("150")).not.toBeInTheDocument();
  });
  it("groups confirmed file edits and keeps context in the Inspector tab", async () => {
    const turn = savedTurn();
    turn.contextWindow = 1000;
    turn.steps[0].tools = [
      { ...turn.steps[0].tools[0], name: "edit", args: { path: "src/main.ts", oldText: "old", newText: "new\nline" }, output: "Alteração salva." },
      { ...turn.steps[0].tools[0], id: "failed", name: "write", args: { path: "missing.txt" }, status: "error", output: "" },
    ];
    vi.mocked(invoke).mockImplementation(async command => command === "get_agent_file_changes" ? [{ path: "src/main.ts", additions: 2, deletions: 1, base: "conversation" }] : []);
    render(<Inspector library={populatedLibrary()} chat={{ ...emptyChat(), turns: [turn], fileChanges: [{ path: "src/main.ts", additions: 2, deletions: 1, base: "conversation" }] }} />);
    expect(screen.getByRole("tab", { name: "Inspector" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Explorer" })).toHaveAttribute("aria-selected", "false");
    const file = await screen.findByRole("button", { name: "Alterações em src/main.ts" });
    expect(file).toHaveTextContent("+2");
    expect(file).toHaveTextContent("−1");
    expect(screen.queryByText("missing.txt")).not.toBeInTheDocument();
    expect(screen.queryByText("Alteração salva.")).not.toBeInTheDocument();
    const footer = screen.getByRole("contentinfo", { name: "Contexto da conversa" });
    expect(Number(within(footer).getByRole("progressbar").getAttribute("aria-valuenow"))).toBeCloseTo(19.4);
    expect(within(footer).queryByText(/Estimativa:/)).not.toBeInTheDocument();
    expect(screen.queryByText("/projects/jarvis")).not.toBeInTheDocument();
    expect(footer).toBeInTheDocument();
  });

  it("shows empty activity sections and an unknown context without fabricating data", async () => {
    render(<Inspector library={populatedLibrary()} />);
    expect(screen.getByText("Nenhuma alteração pendente.")).toBeInTheDocument();
    expect(screen.getByText("Nenhuma tarefa registrada nesta solicitação.")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.queryByText("Primeira conversa")).not.toBeInTheDocument();
    expect(screen.queryByText("Conversa do trabalho")).not.toBeInTheDocument();
  });

  it("clears stale files and context when another conversation is selected", async () => {
    const turn = savedTurn();
    turn.contextWindow = 1000;
    turn.steps[0].tools[0].name = "write";
    const chat = { ...emptyChat(), turns: [turn] };
    const { rerender } = render(<Inspector library={populatedLibrary()} chat={chat} />);
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
    rerender(<Inspector library={emptyLibrary()} chat={chat} />);
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Alterações em/ })).not.toBeInTheDocument();
    expect(screen.getByText("Nenhuma tarefa registrada nesta solicitação.")).toBeInTheDocument();
  });
});
