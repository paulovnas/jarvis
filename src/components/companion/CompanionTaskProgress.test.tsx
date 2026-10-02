import { useState } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import type { DirectTask } from "@/core/chat";
import { CompanionTaskProgress } from "./CompanionTaskProgress";

function Progress({ tasks }: { tasks: DirectTask[] }) {
  const [open, setOpen] = useState(false);
  return <CompanionTaskProgress tasks={tasks} active open={open} onOpenChange={setOpen}><span>Trabalhando</span></CompanionTaskProgress>;
}

it("keeps large plans compact and exposes every task with the keyboard", async () => {
  const tasks: DirectTask[] = Array.from({ length: 20 }, (_, index) => ({ id: `task-${index}`, title: `Tarefa ${index + 1}`, status: index === 19 ? "blocked" : "pending" }));
  const user = userEvent.setup();
  render(<Progress tasks={tasks} />);
  const summary = screen.getByRole("button", { name: /Tarefas: 0 de 20 tarefas concluídas/ });
  expect(summary).toHaveTextContent("0/20");
  expect(summary).toHaveTextContent("+14");
  expect(summary).toHaveAccessibleName(expect.stringContaining("1 bloqueada"));
  await user.tab();
  expect(summary).toHaveFocus();
  await user.keyboard("{Enter}");
  const list = screen.getByRole("list", { name: "Tarefas do agente" });
  expect(within(list).getAllByRole("listitem")).toHaveLength(20);
  expect(within(list).getByText("Tarefa 20")).toBeVisible();
  expect(within(list).getByText("Bloqueada")).toBeVisible();
  await user.keyboard(" ");
  expect(summary).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByRole("list", { name: "Tarefas do agente" })).not.toBeInTheDocument();
});
