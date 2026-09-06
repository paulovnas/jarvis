import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { bead } from "@/test/dashboard-fixtures";
import { EpicPlans } from "./EpicPlans";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const epic = bead({ id: "epic", issue_type: "epic", title: "Integrar autenticação", status: "in_progress" });

describe("Epic plans", () => {
  beforeEach(() => call.mockReset());
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
    render(<EpicPlans projectId="p1" onOpenKanban={open} />);
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
    await user.click(within(dialog).getByRole("button", { name: "Ver mais detalhes" }));
    expect(open).toHaveBeenCalledWith("p1");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("removes a closed epic and its open modal after refreshing on focus", async () => {
    call.mockResolvedValue([epic]);
    render(<EpicPlans projectId="p1" />);
    fireEvent.click(await screen.findByRole("button", { name: `Plano: ${epic.title}` }));
    await screen.findByRole("dialog", { name: epic.title });
    call.mockResolvedValue([{ ...epic, status: "closed" }]);
    fireEvent(window, new Event("focus"));
    await screen.findByText("Nenhum plano em aberto.");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    call.mockResolvedValue([epic]);
    fireEvent(window, new Event("focus"));
    await screen.findByRole("button", { name: `Plano: ${epic.title}` });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("does not leak a late response from another project", async () => {
    let resolve!: (value: unknown) => void;
    call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    const view = render(<EpicPlans key="p1" projectId="p1" />);
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_project_beads", { projectId: "p1" }));
    call.mockResolvedValue([]);
    view.rerender(<EpicPlans key="p2" projectId="p2" />);
    await act(async () => resolve([epic]));
    await screen.findByText("Nenhum plano em aberto.");
    expect(screen.queryByText(epic.title)).not.toBeInTheDocument();
  });
});
