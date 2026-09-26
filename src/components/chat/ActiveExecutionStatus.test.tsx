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

it("does not confuse an inactive clock with a pending user response", () => {
  const turn = { ...savedTurn(), status: "running" as const, durationMs: 0, activeSince: null };
  const step = { ...turn.steps[0], summary: "", text: "", tools: [], coreActivities: [{ component: "beads" as const, action: "prepare", status: "applied" as const, summary: "Tarefas disponíveis", sources: [], durationMs: 0 }] };
  const { rerender } = render(<ActiveExecutionStatus turn={{ ...turn, steps: [] }} />);
  expect(screen.getByRole("status")).toHaveTextContent("Preparando execução…");

  rerender(<ActiveExecutionStatus turn={{ ...turn, steps: [step] }} />);
  expect(screen.getByRole("status")).toHaveTextContent("Trabalhando…");
  expect(screen.getByRole("status")).not.toHaveClass("text-onedark-yellow");

  rerender(<ActiveExecutionStatus turn={turn} />);
  expect(screen.getByRole("status")).toHaveTextContent("Verificando o projeto");

  rerender(<ActiveExecutionStatus turn={{ ...turn, steps: [{ ...step, tools: [{ id: "ask", name: "ask_user", args: {}, status: "running", output: "", durationMs: 0 }] }] }} />);
  expect(screen.getByRole("status")).toHaveTextContent("Trabalhando…");

  rerender(<ActiveExecutionStatus turn={{ ...turn, steps: [{ ...step, retry: { attempt: 2, maxAttempts: 5, retryAt: 0, message: "Falha temporária" } }] }} />);
  expect(screen.getByRole("status")).toHaveTextContent("Reconectando 2/5");
});

it("distinguishes delegated work from a pending user response", () => {
  const turn = { ...savedTurn(), status: "running" as const, activeSince: null };
  turn.steps[0].tools = [{ id: "wait", name: "hub_wait", args: {}, status: "running", output: "", durationMs: 0 }];
  const { rerender } = render(<ActiveExecutionStatus turn={turn} />);
  expect(screen.getByRole("status")).toHaveTextContent("Aguardando agentes");
  rerender(<ActiveExecutionStatus turn={turn} waitingForUser />);
  expect(screen.getByRole("status")).toHaveTextContent("Aguardando sua resposta");
});

it("freezes the visible work time while waiting and resumes without adding absent hours", () => {
  vi.useFakeTimers();
  try {
    const turn = { ...savedTurn(), status: "running" as const, durationMs: 20_000, activeSince: null };
    const { rerender } = render(<ActiveExecutionStatus turn={turn} waitingForUser />);
    act(() => { vi.advanceTimersByTime(9 * 3_600_000); });
    expect(screen.getByLabelText("Tempo total da execução")).toHaveTextContent(/^20s$/);
    expect(screen.getByRole("status")).toHaveTextContent("Aguardando sua resposta");
    rerender(<ActiveExecutionStatus turn={{ ...turn, activeSince: Date.now() }} />);
    act(() => { vi.advanceTimersByTime(5_000); });
    expect(screen.getByLabelText("Tempo total da execução")).toHaveTextContent(/^25s$/);
    expect(screen.getByRole("status")).toHaveTextContent("Verificando o projeto");
  } finally {
    vi.useRealTimers();
  }
});
