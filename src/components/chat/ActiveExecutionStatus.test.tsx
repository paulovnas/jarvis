import { act, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { savedTurn } from "@/test/chat-fixtures";
import { ActiveExecutionStatus } from "./ActiveExecutionStatus";

it("keeps the latest thought and elapsed time visible in the active execution dock", () => {
  const turn = {
    ...savedTurn(),
    createdAt: Date.now() - 101_000,
    durationMs: 2_000,
    status: "running" as const,
    steps: [{ durationMs: 2_000, summary: "Validando o resultado", text: "", tools: [{ id: "read-1", name: "read", args: {}, status: "completed" as const, output: "ok", durationMs: 12 }], usage: null }],
  };
  const { rerender } = render(<ActiveExecutionStatus turn={turn} />);
  const status = screen.getByTestId("active-execution-status");
  expect(within(status).getByRole("status")).toHaveTextContent("Validando o resultado");
  expect(within(status).getByLabelText("Tempo total da execução")).toHaveTextContent(/1m 4[1-3]s/);
  expect(status).toHaveTextContent("1 ação");
  expect(status).not.toHaveClass("rounded-lg", "border", "bg-card/95");

  rerender(<ActiveExecutionStatus turn={{ ...turn, steps: [{ ...turn.steps[0], retry: { attempt: 2, maxAttempts: 5, retryAt: 0, message: "Falha temporária" } }] }} />);
  expect(within(status).getByRole("status")).toHaveTextContent("Reconectando 2/5");
});

it("freezes the visible work time while waiting and resumes without adding absent hours", () => {
  vi.useFakeTimers();
  try {
    const turn = { ...savedTurn(), status: "running" as const, durationMs: 20_000, activeSince: null };
    const { rerender } = render(<ActiveExecutionStatus turn={turn} />);
    act(() => { vi.advanceTimersByTime(9 * 3_600_000); });
    expect(screen.getByLabelText("Tempo total da execução")).toHaveTextContent(/^20s$/);
    expect(screen.getByRole("status")).toHaveTextContent("Aguardando sua resposta");
    rerender(<ActiveExecutionStatus turn={{ ...turn, activeSince: Date.now() }} />);
    act(() => { vi.advanceTimersByTime(5_000); });
    expect(screen.getByLabelText("Tempo total da execução")).toHaveTextContent(/^25s$/);
  } finally {
    vi.useRealTimers();
  }
});
