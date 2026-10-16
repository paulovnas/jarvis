import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { emptyLibrary, populatedLibrary } from "@/test/library-fixtures";
import { Inspector } from "./Inspector";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { DesktopLayoutProvider } from "./DesktopLayoutProvider";
import { DEFAULT_DESKTOP_LAYOUT, type DesktopLayout } from "@/core/desktop-layout";
import type { DesignerSession } from "./DesignerPanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockResolvedValue([]); });

describe("Inspector", () => {
  it("identifies the Muse context owner without offering native compaction", async () => {
    const chat = { ...emptyChat(), latestOptions: { ...savedTurn().options, executor: "muse" as const } };
    render(<DesktopLayoutProvider><Inspector library={populatedLibrary()} chat={chat} /></DesktopLayoutProvider>);
    const context = await screen.findByLabelText("Contexto da conversa");
    expect(context).toHaveTextContent("Contexto gerenciado pelo Muse Code");
    expect(within(context).queryByRole("button", { name: "Compactar contexto" })).not.toBeInTheDocument();
  });
  it("disables project panels for global chats and restores them in a project", async () => {
    const stored = populatedLibrary();
    const global = { ...stored, conversations: [...stored.conversations, { id: "global", projectId: null, title: "Global", createdAt: 1 }], selection: { workspaceId: "w1", projectId: null, conversationId: "global" } };
    vi.mocked(invoke).mockImplementation(async command => command === "get_desktop_layout" ? { ...DEFAULT_DESKTOP_LAYOUT, inspectorTab: "designer" } : []);
    const view = render(<DesktopLayoutProvider><Inspector library={global} /></DesktopLayoutProvider>);
    await screen.findByRole("button", { name: "Inspector" });
    for (const name of ["GitHub", "Explorer", "Designer"]) expect(screen.getByRole("button", { name })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Inspector" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("region", { name: "Designer e Impeccable Live" })).not.toBeInTheDocument();
    view.rerender(<DesktopLayoutProvider><Inspector library={stored} /></DesktopLayoutProvider>);
    for (const name of ["GitHub", "Explorer", "Designer"]) expect(screen.getByRole("button", { name })).not.toBeDisabled();
    await waitFor(() => expect(screen.getByRole("button", { name: "Designer" })).toHaveAttribute("aria-pressed", "true"));
  });
  it("keeps a collapsed activity section across tab changes and a restored layout", async () => {
    const user = userEvent.setup();
    let layout: DesktopLayout = DEFAULT_DESKTOP_LAYOUT;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_desktop_layout") return layout;
      if (command === "save_desktop_layout") layout = (args as { layout: DesktopLayout }).layout;
      return [];
    });
    const first = render(<DesktopLayoutProvider><Inspector library={populatedLibrary()} /></DesktopLayoutProvider>);
    await user.click(await screen.findByRole("button", { name: "Tarefas" }));
    await waitFor(() => expect(layout.activitySections.Tarefas).toBe(false));
    await waitFor(() => expect(screen.queryByText("Nenhuma tarefa registrada nesta solicitação.")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Designer" }));
    await user.click(screen.getByRole("button", { name: "Inspector" }));
    expect(screen.getByRole("button", { name: "Tarefas" })).toHaveAttribute("aria-expanded", "false");
    first.unmount();
    render(<DesktopLayoutProvider><Inspector library={populatedLibrary()} /></DesktopLayoutProvider>);
    expect(await screen.findByRole("button", { name: "Tarefas" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Nenhuma tarefa registrada nesta solicitação.")).not.toBeInTheDocument();
  });
  it("persists the Designer tab and never shows another conversation's Live requests", async () => {
    const user = userEvent.setup();
    let layout: DesktopLayout = DEFAULT_DESKTOP_LAYOUT;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_desktop_layout") return layout;
      if (command === "save_desktop_layout") layout = (args as { layout: DesktopLayout }).layout;
      return [];
    });
    const designer: DesignerSession = {
      conversationId: "other", canResume: false, attention: false, onResume: vi.fn(async () => {}), onShowChat: vi.fn(),
      live: { status: { conversationId: "other", state: "ready", phase: "running", actionId: "other-action", url: "http://localhost:5173/", tabId: "other-page", error: null, setupNeeded: null, tasks: [{ requestId: "other:1", actionId: "other-action", revision: 1, content: "Pedido de outra conversa", status: "pending", beadId: null, turnId: null, createdAt: 1791568800000 }] }, active: true, loaded: true, busy: false, toggle: vi.fn(async () => {}), resume: vi.fn(async () => {}), editTask: vi.fn(async () => null), removeTask: vi.fn(async () => null), resumeTask: vi.fn(async () => null), cancelAction: vi.fn(async () => null) },
    };
    const first = render(<DesktopLayoutProvider><Inspector library={populatedLibrary()} chat={emptyChat()} designer={designer} /></DesktopLayoutProvider>);
    await user.click(await screen.findByRole("button", { name: "Designer" }));
    await waitFor(() => expect(layout.inspectorTab).toBe("designer"));
    expect(screen.queryByText("Pedido de outra conversa")).not.toBeInTheDocument();
    expect(screen.getByText("Abra uma conversa para acompanhar as alterações.")).toBeVisible();
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "get_impeccable_live" || command === "start_impeccable_live")).toBe(false);
    first.unmount();
    render(<DesktopLayoutProvider><Inspector library={populatedLibrary()} chat={emptyChat()} /></DesktopLayoutProvider>);
    expect(await screen.findByRole("button", { name: "Designer" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("region", { name: "Designer e Impeccable Live" })).toBeVisible();
  });
  it("keeps changed files visible without a publication shortcut in idle and active chats", async () => {
    vi.mocked(invoke).mockImplementation(async command => command === "get_agent_file_changes" ? [{ path: "src/main.ts", additions: 1, deletions: 0, base: "conversation" }] : []);
    const chat = { ...emptyChat(), turns: [savedTurn()] };
    const view = render(<Inspector library={populatedLibrary()} chat={chat} />);
    expect(await screen.findByRole("button", { name: "Alterações em src/main.ts" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Publicar" })).not.toBeInTheDocument();
    view.rerender(<Inspector library={populatedLibrary()} chat={{ ...chat, activeTurnId: chat.turns[0].id }} />);
    expect(screen.getByRole("button", { name: "Alterações em src/main.ts" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Publicar" })).not.toBeInTheDocument();
  });
  it("shows the GitHub subagent only while publication is the latest conversation turn", () => {
    const chat = emptyChat();
    const publication = savedTurn();
    publication.options.workflow = "publication";
    const workflow = { data: { conversationId: chat.conversationId, revision: 1, flow: "publication" as const, agents: [], validation: null }, error: null, loading: false, retry: vi.fn() };
    const view = render(<Inspector library={populatedLibrary()} chat={{ ...chat, turns: [publication] }} workflow={workflow} />);
    expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Tarefas/ })).not.toBeInTheDocument();

    const next = savedTurn();
    next.id = "next-turn";
    next.options.workflow = "standard";
    view.rerender(<Inspector library={populatedLibrary()} chat={{ ...chat, turns: [publication, next] }} workflow={workflow} />);
    expect(screen.getByRole("button", { name: /Tarefas/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Subagentes" })).not.toBeInTheDocument();
  });
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

    turn.options.workflow = "video";
    view.rerender(<Inspector library={populatedLibrary()} chat={{ ...directChat, turns: [{ ...turn }] }} />);
    expect(screen.getByRole("button", { name: /Tarefas.*2/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Subagentes" })).not.toBeInTheDocument();

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

  it("shows manual validation only when enabled for the selected coordinated workflow", async () => {
    const chat = emptyChat(); const library = populatedLibrary(); const turn = savedTurn();
    turn.options.workflow = "planned";
    turn.options.manualValidation = true;
    const coordinatedChat = { ...chat, turns: [turn] };
    const workflow = { data: { conversationId: chat.conversationId, revision: 1, flow: "planned" as const, agents: [], validation: null }, error: null, loading: false, retry: vi.fn() };
    const view = render(<Inspector library={library} chat={coordinatedChat} workflow={workflow} />);
    expect(screen.getByRole("button", { name: "Validação" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
    view.rerender(<Inspector library={library} chat={coordinatedChat} workflow={{ ...workflow, data: { ...workflow.data, flow: "complete" } }} />);
    expect(screen.getByRole("button", { name: "Validação" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
    for (const flow of ["standard", "designer", "video"] as const) {
      view.rerender(<Inspector library={library} chat={coordinatedChat} workflow={{ ...workflow, data: { ...workflow.data, flow } }} />);
      expect(screen.queryByRole("button", { name: "Validação" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Subagentes" })).not.toBeInTheDocument();
    }
    turn.options.workflow = "custom";
    turn.options.customWorkflowId = "b".repeat(32);
    view.rerender(<Inspector library={library} chat={{ ...chat, turns: [{ ...turn }] }} workflow={{ ...workflow, data: { ...workflow.data, flow: "custom" } }} />);
    expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Validação" })).toBeVisible();
    delete turn.options.customWorkflowId;
    turn.options.customAgentId = "a".repeat(32);
    view.rerender(<Inspector library={library} chat={{ ...chat, turns: [turn] }} workflow={{ ...workflow, data: { ...workflow.data, flow: "custom" } }} />);
    expect(screen.getByRole("button", { name: /Tarefas/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Subagentes" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Validação" })).not.toBeInTheDocument();
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
    expect(screen.getByRole("button", { name: "Inspector" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Explorer" })).toHaveAttribute("aria-pressed", "false");
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

  it("shows configured repositories and their local Git state in the GitHub tab", async () => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "get_agent_file_changes") return [];
      if (command === "get_project_repositories") return [{ id: "repo", projectId: "p1", path: "backend", directory: "/projects/jarvis/backend", name: "Backend", description: "API principal", branch: "feature/pagamentos", upstream: "origin/feature/pagamentos", ahead: 2, behind: 1, staged: 1, unstaged: 3, untracked: 2, remoteUrl: "https://github.com/example/backend.git", available: true, error: null, createdAt: 1, updatedAt: 1 }];
      return [];
    });
    const user = userEvent.setup();
    render(<Inspector library={populatedLibrary()} chat={emptyChat()} />);
    await user.click(screen.getByRole("button", { name: "GitHub" }));
    expect(screen.getByRole("button", { name: "GitHub" })).toHaveAttribute("aria-pressed", "true");
    const repository = await screen.findByRole("article", { name: "GitHub Backend" });
    expect(repository).toHaveTextContent("feature/pagamentos");
    expect(repository).toHaveTextContent("2");
    expect(repository).toHaveTextContent("1");
    expect(repository).toHaveTextContent("3");
    expect(repository).toHaveTextContent("2");
    expect(repository).toHaveTextContent("origin/feature/pagamentos");
    expect(invoke).toHaveBeenCalledWith("get_project_repositories", { projectId: "p1", includeDefault: true });
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_project_repositories")).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "Inspector" }));
    await user.click(screen.getByRole("button", { name: "GitHub" }));
    await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_project_repositories")).toHaveLength(2));
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

it.each(["standard", "designer", "image_generator"] as const)("shows a delegated image specialist in a direct %s conversation", flow => {
  const chat = emptyChat();
  const turn = savedTurn();
  turn.options.workflow = flow;
  const child = {
    id: "image-worker", parentId: "main", role: "image_generator" as const,
    title: "Preparar quatro banners", status: "running" as const, createdAt: 1, updatedAt: 2, startedAt: 1, durationMs: 2000,
    currentThought: "Preparando as variações", attempts: 1, options: turn.options, beadId: null,
    handoff: null, error: null, activeTurnId: "image-turn", pendingApproval: null, pendingQuestion: null,
  };
  const workflow = { data: { conversationId: chat.conversationId, revision: 1, flow, agents: [child], validation: null }, error: null, loading: false, retry: vi.fn() };
  render(<Inspector library={populatedLibrary()} chat={{ ...chat, turns: [turn] }} workflow={workflow} />);
  expect(screen.getByRole("button", { name: "Subagentes" })).toBeVisible();
  expect(screen.getByText("Gerador de imagens")).toBeVisible();
  expect(screen.getByText("Preparar quatro banners")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Plano" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Validação" })).not.toBeInTheDocument();
});
