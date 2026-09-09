import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DirectTask } from "@/core/chat";
import { DirectTasks } from "./DirectTasks";

const tasks: DirectTask[] = [
  { id: "queued", title: "Analisar os arquivos", status: "pending" },
  { id: "active", title: "Implementar o recurso", status: "in_progress" },
  { id: "done", title: "Confirmar o contrato", status: "completed" },
  { id: "blocked", title: "Aguardar serviço externo", status: "blocked" },
];

describe("DirectTasks", () => {
  it("shows the ordered task states and completed progress", () => {
    const { container } = render(<DirectTasks tasks={tasks} active flow="standard" />);
    expect(screen.getByRole("list", { name: "Tarefas do agente" })).toBeVisible();
    expect(screen.getAllByRole("listitem").map(item => item.textContent)).toEqual([
      "Analisar os arquivosPendente",
      "Implementar o recursoEm andamento",
      "Confirmar o contratoConcluída",
      "Aguardar serviço externoBloqueada",
    ]);
    expect(screen.getByText("1/4")).toBeVisible();
    expect(screen.getByRole("progressbar", { name: "Progresso das tarefas" })).toHaveAttribute("aria-valuenow", "25");
    expect(screen.getByText("Confirmar o contrato")).toHaveClass("line-through");
    expect(screen.getByText("Analisar os arquivos")).not.toHaveClass("line-through");
    expect(container.querySelector('[data-status="in_progress"]')).toHaveAttribute("data-working", "true");
    expect(container.querySelector('[data-status="blocked"]')).toBeInTheDocument();
    const { container: idle } = render(<DirectTasks tasks={tasks} active={false} flow="designer" />);
    expect(idle.querySelector('[data-status="in_progress"]')).toHaveAttribute("data-working", "false");
  });

  it("distinguishes an active agent still organizing from a finished request", () => {
    const view = render(<DirectTasks tasks={[]} active flow="designer" />);
    expect(screen.getByRole("status")).toHaveTextContent("O agente ainda está organizando o trabalho.");
    view.rerender(<DirectTasks tasks={[]} active={false} flow="designer" />);
    expect(screen.getByRole("status")).toHaveTextContent("Nenhuma tarefa registrada nesta solicitação.");
  });
});
