import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ToolApproval } from "./ToolApproval";

it("explains and submits approval for closing a terminal owned by another context", async () => {
  const user = userEvent.setup();
  const answer = vi.fn(async () => true);
  render(<ToolApproval
    tool={{
      id: "close-1",
      name: "terminal_close",
      args: { id: "terminal-user", reason: "A verificação terminou." },
      status: "pending",
      output: "",
      durationMs: 0,
    }}
    projectPath="/projeto"
    onAnswer={answer}
  />);

  expect(screen.getByText("Autorizar fechamento de terminal?")).toBeVisible();
  expect(screen.getByText("Terminal")).toBeVisible();
  expect(screen.getByText("terminal-user")).toBeVisible();
  expect(screen.getByText("Motivo")).toBeVisible();
  expect(screen.getByText("A verificação terminou.")).toBeVisible();

  await user.click(screen.getByRole("button", { name: "Autorizar uma vez" }));
  expect(answer).toHaveBeenCalledWith(true);
});
