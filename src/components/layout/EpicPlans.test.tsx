import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { bead } from "@/test/dashboard-fixtures";
import { EpicPlans } from "./EpicPlans";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const epic = bead({ id: "epic", issue_type: "epic", title: "Integrar autenticação", status: "in_progress" });

describe("Epic plans", () => {
  beforeEach(() => call.mockReset());
  afterEach(() => vi.useRealTimers());
  it("shows unfinished epics, summarizes all child relations and opens the project Kanban", async () => {
    call.mockResolvedValue([
      epic,
      bead({ id: "blocked", title: "Plano bloqueado", issue_type: "epic", status: "blocked" }),
      bead({ id: "deferred", title: "Plano adiado", issue_type: "epic", status: "deferred" }),
      bead({ id: "closed", title: "Plano concluído", issue_type: "epic", status: "closed" }),
      bead({ id: "a", title: "Conectar", parent: "epic", status: "closed" }),
      bead({ id: "b", title: "Desconectar", dependencies: [{ id: "epic", title: epic.title, status: "in_progress", dependency_type: "parent-child" }] }),
      bead({ id: "unrelated", title: "Outra tarefa", dependencies: [{ id: "epic", title: epic.title, status: "in_progress", dependency_type: "blocks" }] }),
    ]);
    const open = vi.fn(); const user = userEvent.setup();
    render(<EpicPlans projectId="p1" conversationId="c1" onOpenKanban={open} />);
    expect(screen.getByRole("status", { name: "Carregando planos" })).toBeInTheDocument();
    const card = await screen.findByRole("button", { name: `Plano: ${epic.title}` });
    expect(card).toHaveTextContent("1/2 tarefas");
    expect(screen.getByRole("button", { name: "Plano: Plano bloqueado" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Plano: Plano adiado" })).toBeInTheDocument();
    expect(screen.queryByText("Plano concluído")).not.toBeInTheDocument();
    expect(screen.queryByText("Outra tarefa")).not.toBeInTheDocument();
    await user.click(card);
    const dialog = await screen.findByRole("dialog", { name: epic.title });
    expect(within(dialog).getByText("Conectar")).toBeInTheDocument();
    expect(within(dialog).getByText("Desconectar")).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /Editar|Excluir/ })).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Ver mais detalhes" }).parentElement).toHaveClass("mx-0", "mb-0", "px-5", "pb-6");
    await user.click(within(dialog).getByRole("button", { name: "Ver mais detalhes" }));
    expect(open).toHaveBeenCalledWith("p1");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("keeps plans scoped to the selected conversation and closes its open tasks after confirmation", async () => {
    const child = bead({ id: "task", title: "Implementar", parent: epic.id, status: "blocked" });
    const other = bead({ id: "other", title: "Plano de outro chat", issue_type: "epic", metadata: { jarvis_conversation: "c2" } });
    call.mockImplementation(async command => {
      if (command === "get_project_beads") return [epic, child, other];
      if (command === "close_conversation_plan") return [{ ...epic, status: "closed" }, { ...child, status: "closed" }, other];
      return [];
    });
    const user = userEvent.setup();
    render(<EpicPlans projectId="p1" conversationId="c1" />);
    expect(await screen.findByRole("button", { name: `Plano: ${epic.title}` })).toBeInTheDocument();
    expect(screen.queryByText(other.title)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: `Plano: ${epic.title}` }));
    await user.click(screen.getByRole("button", { name: "Encerrar plano" }));
    const confirmation = screen.getByRole("alertdialog", { name: "Encerrar este plano?" });
    expect(confirmation).toHaveTextContent("1 tarefas ainda abertas");
    await user.click(within(confirmation).getByRole("button", { name: "Encerrar plano" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("close_conversation_plan", {
      projectId: "p1", conversationId: "c1", issueId: epic.id,
    }));
  });
  it("removes a closed epic and its open modal after refreshing on focus", async () => {
    call.mockResolvedValue([epic]);
    render(<EpicPlans projectId="p1" conversationId="c1" />);
    fireEvent.click(await screen.findByRole("button", { name: `Plano: ${epic.title}` }));
    await screen.findByRole("dialog", { name: epic.title });
    call.mockResolvedValue([{ ...epic, status: "closed" }]);
    vi.useFakeTimers();
    await act(async () => { fireEvent(window, new Event("focus")); });
    expect(screen.getByText("FINALIZADO")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await act(async () => { vi.advanceTimersByTime(1000); fireEvent(window, new Event("focus")); });
    await act(async () => { vi.advanceTimersByTime(1200); });
    expect(screen.getByText("Nenhum plano em aberto.")).toBeInTheDocument();
    await act(async () => { fireEvent(window, new Event("focus")); });
    expect(screen.queryByText("FINALIZADO")).not.toBeInTheDocument();
    vi.useRealTimers();
    call.mockResolvedValue([epic]);
    fireEvent(window, new Event("focus"));
    await screen.findByRole("button", { name: `Plano: ${epic.title}` });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("does not celebrate missing or initially closed epics and cancels the animation when reopened", async () => {
    call.mockResolvedValue([epic, { ...epic, id: "old", title: "Antigo", status: "closed" }]);
    render(<EpicPlans projectId="p1" conversationId="c1" />);
    await screen.findByRole("button", { name: `Plano: ${epic.title}` });
    expect(screen.queryByText("FINALIZADO")).not.toBeInTheDocument();
    call.mockResolvedValue([]);
    fireEvent(window, new Event("focus"));
    await screen.findByText("Nenhum plano em aberto.");
    expect(screen.queryByText("FINALIZADO")).not.toBeInTheDocument();
    call.mockResolvedValue([epic]);
    fireEvent(window, new Event("focus"));
    await screen.findByRole("button", { name: `Plano: ${epic.title}` });
    vi.useFakeTimers();
    call.mockResolvedValue([{ ...epic, status: "closed" }]);
    await act(async () => { fireEvent(window, new Event("focus")); });
    expect(screen.getByText("FINALIZADO")).toBeInTheDocument();
    call.mockResolvedValue([epic]);
    await act(async () => { fireEvent(window, new Event("focus")); });
    await act(async () => { vi.advanceTimersByTime(2500); });
    expect(screen.queryByText("FINALIZADO")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Plano: ${epic.title}` })).toBeInTheDocument();
  });
  it("does not leak a late response from another project", async () => {
    let resolve!: (value: unknown) => void;
    call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    const view = render(<EpicPlans key="p1" projectId="p1" conversationId="c1" />);
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_project_beads", { projectId: "p1" }));
    call.mockResolvedValue([]);
    view.rerender(<EpicPlans key="p2" projectId="p2" conversationId="c1" />);
    await act(async () => resolve([epic]));
    await screen.findByText("Nenhum plano em aberto.");
    expect(screen.queryByText(epic.title)).not.toBeInTheDocument();
  });
});
