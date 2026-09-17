import { render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import { savedTurn } from "@/test/chat-fixtures";
import { ActiveExecutionStatus } from "./ActiveExecutionStatus";

it("keeps the latest thought and elapsed time visible in the active execution dock", () => {
  const turn = {
    ...savedTurn(),
    createdAt: Date.now() - 101_000,
    durationMs: 2_000,
    status: "running" as const,
    steps: [{ durationMs: 2_000, summary: "Validando o resultado", text: "", tools: [], usage: null }],
  };
  const { rerender } = render(<ActiveExecutionStatus turn={turn} />);
  const status = screen.getByTestId("active-execution-status");
  expect(within(status).getByRole("status")).toHaveTextContent("Validando o resultado");
  expect(within(status).getByLabelText("Tempo total da execução")).toHaveTextContent(/1m 4[1-3]s/);

  rerender(<ActiveExecutionStatus turn={{ ...turn, steps: [{ ...turn.steps[0], retry: { attempt: 2, maxAttempts: 5, retryAt: 0, message: "Falha temporária" } }] }} />);
  expect(within(status).getByRole("status")).toHaveTextContent("Reconectando 2/5");
});
