import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { populatedLibrary } from "@/test/library-fixtures";
import { useChat } from "@/hooks/use-chat";
import { ChatArea } from "./ChatArea";
import type { HistoryPage } from "@/core/chat";
import type { LearningSnapshot } from "@/core/project-learning";
import { clearChatStore, updateChatSnapshot } from "@/core/chat-store";
import { TurnBody, type LatestVisibility } from "./Transcript";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
function page(start: number, id = "c1"): HistoryPage {
  return { conversationId: id, history: { start, total: 100 }, compactions: [], navigation: [0, 10, 20, 30, 40, 50, 60, 70, 80, 99].map(index => ({ id: `t${index}`, index, createdAt: 1, user: `Pedido ${index}`, assistant: `Resumo ${index}` })), turns: Array.from({ length: 20 }, (_, i) => ({ ...savedTurn(), id: `t${start + i}`, user: `Pedido ${start + i}`, steps: [{ ...savedTurn().steps[0], text: `Resposta ${start + i}`, summary: "", tools: [] }] })) };
}
function Harness({ id = "c1", onLatestVisibility }: { id?: string; onLatestVisibility?: LatestVisibility }) {
  const library = populatedLibrary(); library.conversations[0].id = id; library.selection.conversationId = id;
  return <ChatArea onLatestVisibility={onLatestVisibility} library={library} chat={useChat(id)} modelGroups={[{ provider: "Codex", models: [{ value: "openai-codex-pessoal/model", label: "Modelo", reasoningLevels: ["medium"], defaultReasoningLevel: "medium" }] }]} />;
}
describe("lazy transcript navigation", () => {
  beforeEach(() => { clearChatStore(); call.mockReset(); vi.mocked(listen).mockResolvedValue(() => {}); });
  it("retains the work disclosure when a saved turn contains only native Core activity", async () => {
    const user = userEvent.setup();
    const turn = savedTurn();
    turn.steps = [{ ...turn.steps[0], text: "Pronto.", summary: "", tools: [], coreActivities: [{ component: "ponytail", action: "coding_guidance", status: "applied", summary: "Orientações aplicadas", sources: [], durationMs: 0 }] }];
    render(<TurnBody turn={turn} />);
    await user.click(screen.getByRole("button", { name: /Trabalhou por/ }));
    await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
    expect(screen.getByText("Ponytail")).toBeVisible();
    expect(screen.getByText(/Orientações aplicadas/)).toBeVisible();
  });
  it("shows a background learning result below the correct answer without reopening the chat", async () => {
    let changed: EventCallback<string> | undefined;
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (event === "project:learning-changed") changed = callback as EventCallback<string>;
      return () => {};
    });
    const learned: LearningSnapshot = { enabled: true, revision: 0, pending: 0, notice: null, lessons: [] };
    call.mockImplementation(async command => command === "get_project_learning" ? structuredClone(learned) : { ...emptyChat(), turns: [savedTurn()] });
    const user = userEvent.setup();
    render(<Harness />);
    await screen.findByRole("button", { name: "Copiar resposta" });
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_project_learning", { projectId: populatedLibrary().selection.projectId }));
    expect(screen.queryByRole("button", { name: /Aprendizados registrados/ })).not.toBeInTheDocument();
    learned.lessons.push({ id: "spacing", scope: ".", content: "Revise o espaçamento entre botões e badges.", check: "", topics: ["layout"], status: "active", origin: "feedback", revision: 1, updatedAt: 1, evidence: [{ conversationId: "c1", messageId: "turn1", excerpt: "Sempre revise o espaçamento", createdAt: 1 }] });
    await act(async () => changed?.({ event: "project:learning-changed", id: 1, payload: populatedLibrary().selection.projectId! }));
    const trigger = await screen.findByRole("button", { name: /Aprendizados registrados/ });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await user.click(trigger);
    expect(screen.getByText(learned.lessons[0].content)).toBeVisible();
  });
  it("reports reading only at the latest messages and clears visibility when leaving", async () => {
    call.mockResolvedValue({ ...emptyChat(), ...page(80) });
    const onLatestVisibility = vi.fn<LatestVisibility>();
    const view = render(<Harness onLatestVisibility={onLatestVisibility} />);
    await screen.findByText("Resposta 99");
    const viewport = view.container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    Object.defineProperties(viewport, { scrollHeight: { configurable:true, value:1600 }, clientHeight: { configurable:true, value:600 } });
    viewport.scrollTop = 1000; fireEvent.scroll(viewport);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", true);
    viewport.scrollTop = 400; fireEvent.scroll(viewport);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", false);
    viewport.scrollTop = 1000; fireEvent.scroll(viewport);
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", true);
    view.unmount();
    expect(onLatestVisibility).toHaveBeenLastCalledWith("c1", false);
  });
  it("never treats the end of an older history page as the latest message", async () => {
    call.mockResolvedValue({ ...emptyChat(), ...page(0) });
    const onLatestVisibility = vi.fn<LatestVisibility>();
    const view = render(<Harness onLatestVisibility={onLatestVisibility} />);
    await screen.findByText("Resposta 19");
    const viewport = view.container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    Object.defineProperties(viewport, { scrollHeight: { configurable:true, value:1600 }, clientHeight: { configurable:true, value:600 } });
    viewport.scrollTop = 1000; fireEvent.scroll(viewport);
    expect(onLatestVisibility).not.toHaveBeenCalledWith("c1", true);
  });
  it("oculta a navegação com menos de dez trechos", async () => {
    call.mockResolvedValue({ ...emptyChat(), ...page(80), navigation: page(80).navigation.slice(0, 9) });
    render(<Harness />);
    await screen.findByText("Resposta 99");
    expect(screen.queryByRole("navigation", { name: "Navegar pela conversa" })).not.toBeInTheDocument();
  });
  it("starts the topic rail at the latest entry and preserves browsing until interaction ends", async ({ onTestFinished }) => {
    onTestFinished(() => { vi.restoreAllMocks(); });
    vi.spyOn(Element.prototype, "scrollHeight", "get").mockReturnValue(1200);
    vi.spyOn(Element.prototype, "clientHeight", "get").mockReturnValue(240);
    call.mockResolvedValue({ ...emptyChat(), ...page(80) });
    const user = userEvent.setup();
    render(<Harness />);
    const rail = await screen.findByRole("navigation", { name: "Navegar pela conversa" });
    expect(rail.scrollTop).toBe(960);
    const addTopic = () => act(() => {
      updateChatSnapshot("c1", current => {
        if (!current) return current;
        const entries = current.navigation ?? [];
        const index = (entries[entries.length - 1]?.index ?? -1) + 1;
        return { ...current, navigation: [...entries, { id: `t${index}`, index, createdAt: 1, user: `Pedido ${index}`, assistant: "" }] };
      });
    });
    await user.hover(rail);
    rail.scrollTop = 200;
    fireEvent.scroll(rail);
    addTopic();
    expect(rail.scrollTop).toBe(200);
    const topic = screen.getByRole("button", { name: "Ir para interação 1: Pedido 0" });
    act(() => topic.focus());
    await user.unhover(rail);
    addTopic();
    expect(rail.scrollTop).toBe(200);
    act(() => topic.blur());
    addTopic();
    expect(rail.scrollTop).toBe(960);
  });
  it("ignores elastic overscroll but loads older messages on a real upward scroll", async () => {
    call.mockImplementation(async command => command === "get_chat_history" ? page(60) : { ...emptyChat(), ...page(80) });
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    Object.defineProperties(viewport, { scrollHeight: { configurable: true, value: 1600 }, clientHeight: { configurable: true, value: 600 } });
    viewport.scrollTop = 400; fireEvent.scroll(viewport);
    viewport.scrollTop = -40; fireEvent.scroll(viewport);
    viewport.scrollTop = 1100; fireEvent.scroll(viewport);
    expect(call.mock.calls.filter(([command]) => command === "get_chat_history")).toHaveLength(0);
    viewport.scrollTop = 100; fireEvent.scroll(viewport);
    await screen.findByText("Resposta 60");
    expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", before: 80 });
  });
  it("loads only the initial page, jumps by excerpt, and keeps the composer draft", async () => {
    call.mockImplementation(async (command, args) => {
      if (command === "subscribe_chat") return { ...emptyChat(), ...page(80) };
      if (command === "get_chat_history") return page(args && "around" in args ? 0 : 80);
      return [];
    });
    const user = userEvent.setup(); render(<Harness />);
    expect(await screen.findByText("Pedido 99")).toBeInTheDocument();
    expect(screen.queryByText("Resposta 0")).not.toBeInTheDocument();
    await user.type(await screen.findByRole("textbox"), "Meu rascunho");
    const first = screen.getByRole("button", { name: "Ir para interação 1: Pedido 0" });
    act(() => first.focus());
    expect(await screen.findByText("Resumo 0")).toBeInTheDocument();
    await user.click(first);
    expect(await screen.findByText("Resposta 0")).toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", around: 0 });
    expect(screen.queryByText("Resposta 99")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveTextContent("Meu rascunho");
    await user.click(screen.getByRole("button", { name: "Voltar ao presente" }));
    expect(await screen.findByText("Resposta 99")).toBeInTheDocument();
  }, 15000);
  it("discards a history response after switching conversation", async () => {
    let resolve: (value: unknown) => void = () => {};
    call.mockImplementation(async (command, args) => command === "get_chat_history" ? new Promise(done => { resolve = done; }) : { ...emptyChat(args && "conversationId" in args ? String(args.conversationId) : "c1"), ...page(80, args && "conversationId" in args ? String(args.conversationId) : "c1") });
    const user = userEvent.setup(); const { rerender } = render(<Harness />);
    await user.click(await screen.findByRole("button", { name: "Mensagens anteriores" }));
    expect(screen.getByRole("status", { name: "Carregando trecho" })).toBeInTheDocument();
    rerender(<Harness id="c2" />);
    await waitFor(() => expect(call).toHaveBeenCalledWith("subscribe_chat", { conversationId: "c2" }));
    await act(async () => resolve(page(60)));
    expect(screen.queryByText("Resposta 60")).not.toBeInTheDocument();
    expect(await screen.findByText("Resposta 99")).toBeInTheDocument();
  });

  it("offers retry only on the newest failed turn", async () => {
    const older = {
      ...savedTurn(),
      id: "older-failure",
      status: "error" as const,
      error: { code: "provider_retry_exhausted", message: "Falha anterior." },
    };
    const newest = {
      ...savedTurn(),
      id: "newest-failure",
      status: "error" as const,
      error: { code: "provider_retry_exhausted", message: "Falha mais recente." },
    };
    const stored = {
      ...emptyChat(),
      revision: 3,
      turns: [older, newest],
      history: { start: 0, total: 2 },
    };
    call.mockImplementation(async command => command === "get_chat_agent_models" ? {} : stored);

    render(<Harness />);

    expect(await screen.findByText("Falha anterior.")).toBeInTheDocument();
    expect(await screen.findByText("Falha mais recente.")).toBeInTheDocument();
    expect(await screen.findAllByRole("button", { name: "Tentar novamente" })).toHaveLength(1);
  });
});

describe("estado do turno", () => {
  it("keeps the latest running text as an observation and promotes only the completed text to the final answer", async () => {
    const firstStep = {
      ...savedTurn().steps[0],
      summary: "Analisando o projeto",
      text: "Vou inspecionar os arquivos relevantes.",
      tools: [],
    };
    const running = {
      ...savedTurn(),
      status: "running" as const,
      createdAt: Date.now(),
      steps: [firstStep],
    };
    const { rerender } = render(<TurnBody turn={running} />);

    const observation = await screen.findByText("Vou inspecionar os arquivos relevantes.");
    expect(observation.closest("[data-execution-observation]")).not.toBeNull();
    expect(screen.getByLabelText("Atividades da execução atual")).toBeVisible();
    expect(screen.queryByRole("button", { name: /Trabalhou por/ })).not.toBeInTheDocument();

    rerender(<TurnBody turn={{
      ...running,
      status: "completed",
      durationMs: 4_000,
      steps: [firstStep, { ...firstStep, summary: "", text: "A análise foi concluída.", tools: [] }],
    }} />);

    expect(screen.getByRole("button", { name: /Trabalhou por 4s/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Vou inspecionar os arquivos relevantes.")).not.toBeInTheDocument();
    expect(await screen.findByText("A análise foi concluída.")).toBeVisible();
  });

  it("preserves the delivery report when automatic LSP feedback adds a final acknowledgement", async () => {
    const user = userEvent.setup();
    const turn = savedTurn();
    const step = turn.steps[0];
    const report = "Corrigi o descarte da transferência. Os testes passaram; a homologação continua pendente.";
    const acknowledgement = "O LSP não reportou diagnósticos novos.";
    turn.steps = [
      { ...step, text: "Vou investigar a transferência." },
      { ...step, tools: [], summary: "", text: report, coreActivities: [{ component: "lsp", action: "file_diagnostics", status: "applied", summary: "Sem diagnósticos novos", sources: ["app.ts"], durationMs: 1 }] },
      { ...step, tools: [], summary: "", text: acknowledgement },
    ];
    render(<TurnBody turn={turn} />);

    expect(await screen.findByText(report)).toBeVisible();
    expect(screen.getByText(acknowledgement)).toBeVisible();
    expect(screen.queryByText("Vou investigar a transferência.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Salvar resposta em Markdown" }));
    expect(call).toHaveBeenCalledWith("save_markdown_document", expect.objectContaining({ content: `${report}\n\n${acknowledgement}` }));
    await user.click(screen.getByRole("button", { name: /Trabalhou por/ }));
    expect(await screen.findByText("Vou investigar a transferência.")).toBeVisible();
    expect(screen.getAllByText(report)).toHaveLength(1);
    expect(screen.getAllByText(acknowledgement)).toHaveLength(1);
  });

  it("does not promote a report or LSP acknowledgement preceding a later tool call", async () => {
    const turn = savedTurn();
    const step = turn.steps[0];
    turn.steps = [
      { ...step, tools: [], summary: "", text: "A primeira correção foi concluída.", coreActivities: [{ component: "lsp", action: "file_diagnostics", status: "applied", summary: "Sem diagnósticos novos", sources: ["app.ts"], durationMs: 1 }] },
      { ...step, text: "Vou corrigir também a validação." },
      { ...step, tools: [], summary: "", text: "A correção final inclui a validação." },
    ];
    render(<TurnBody turn={turn} />);
    expect(await screen.findByText("A correção final inclui a validação.")).toBeVisible();
    expect(screen.queryByText("A primeira correção foi concluída.")).not.toBeInTheDocument();
    expect(screen.queryByText("Vou corrigir também a validação.")).not.toBeInTheDocument();
  });

  it("apresenta uma pausa do watchdog como retomável, sem tratá-la como falha", () => {
    render(<TurnBody turn={{
      ...savedTurn(),
      status: "interrupted",
      error: {
        code: "progress_paused",
        message: "O histórico foi preservado. Envie uma nova mensagem para continuar.",
      },
    }} />);

    expect(screen.getByText("Execução pausada")).toBeInTheDocument();
    expect(screen.getByText(/histórico foi preservado/i)).toBeInTheDocument();
  });

  it("shows a blocking failure once when the final text repeats the turn error", () => {
    const message = "A observação da aprovação ainda precisa ser incorporada.";
    const turn = savedTurn();
    turn.status = "error";
    turn.error = { code: "publication_blocked", message };
    turn.steps = [{ ...turn.steps[0], summary: "", tools: [], text: message }];

    render(<TurnBody turn={turn} />);

    expect(screen.getByRole("alert")).toHaveTextContent(message);
    expect(screen.getAllByText(message)).toHaveLength(1);
  });

  it("offers to continue a failed turn from the error message", async () => {
    const retry = vi.fn();
    const turn = savedTurn();
    turn.status = "error";
    turn.options.workflow = "publication";
    turn.error = { code: "provider_retry_exhausted", message: "A conexão com o provedor falhou." };

    render(<TurnBody turn={turn} onRetry={retry} />);

    await userEvent.setup().click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(retry).toHaveBeenCalledOnce();
    expect(retry).toHaveBeenCalledWith(turn.id);
  });

  it("continues a failed canvas workflow through the same retry action", async () => {
    const retry = vi.fn();
    const turn = savedTurn();
    turn.status = "error";
    turn.options.workflow = "custom";
    turn.options.customWorkflowId = "custom-flow";
    turn.error = { code: "workflow_failed", message: "Uma etapa falhou." };

    render(<TurnBody turn={turn} onRetry={retry} />);

    await userEvent.setup().click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(retry).toHaveBeenCalledWith(turn.id);
  });
});
