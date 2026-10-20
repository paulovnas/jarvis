import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";

it("keeps failure severity identical before and after expanding the block", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse work={{ durationSeconds: 3, steps: [{
    thinking: "",
    commentary: "",
    tools: [{ id: "read-failed", name: "read", status: "error", args: {}, output: "Falha ao ler o arquivo" }],
  }] }} />);

  expect(screen.getByText("1 ação não concluída")).toBeInTheDocument();
  expect(screen.queryByText("1 aviso")).not.toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: /Trabalhou por 3s/ }));

  expect(screen.getByText("1 ação não concluída")).toBeInTheDocument();
  expect(screen.queryByText("1 aviso")).not.toBeInTheDocument();
  expect(screen.getAllByLabelText("1 falha").length).toBeGreaterThan(0);
});

it("does not present an expected preflight reminder as a failure", async () => {
  const user = userEvent.setup();
  render(<AssistantWorkCollapse work={{ durationSeconds: 4, steps: [{
    thinking: "",
    commentary: "",
    tools: [{
      id: "ctx-reminder",
      name: "ctx_execute",
      status: "error",
      args: {},
      output: "Atualize a lista com update_tasks e mantenha uma tarefa em andamento antes de executar",
    }],
  }] }} />);

  expect(screen.queryByText("1 ação não concluída")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Trabalhou por 4s/ }));
  expect(screen.getAllByLabelText("1 aviso").length).toBeGreaterThan(0);
  expect(screen.queryByLabelText("1 falha")).not.toBeInTheDocument();
});
