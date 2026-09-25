import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { emptyEfficiency } from "@/core/dashboard";
import { projectMetrics } from "@/test/dashboard-fixtures";
import { UsageEfficiency } from "./UsageEfficiency";

describe("Usage efficiency", () => {
  it("does not invent cache hits for old sessions", () => {
    render(<UsageEfficiency metrics={projectMetrics().metrics} />);
    expect(within(screen.getByRole("region", { name: "Cache do provedor" })).getByText("Sem medição")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.queryByText("Tokens gravados no cache")).not.toBeInTheDocument();
  });
  it("summarizes measured benefits and keeps technical details available on demand", async () => {
    const user = userEvent.setup();
    render(<UsageEfficiency metrics={{ ...projectMetrics().metrics, measuredSteps: 8, inputTokens: 5000,
      efficiency: { ...emptyEfficiency, cacheReadTokens: 600, cacheReadInputTokens: 1000, cacheReadRequests: 2,
        cacheWriteTokens: 200, cacheWriteRequests: 1, auxiliaryRequests: 2,
        indexedOutputs: 3, originalBytes: 10000, retainedBytes: 1000, contextSearches: 7,
        localReadReuses: 4, localReadOriginalBytes: 8000, localReadRetainedBytes: 200,
        loopSteers: 2, loopAvoidedCalls: 1 } }} />);
    const cache = within(screen.getByRole("region", { name: "Cache do provedor" }));
    expect(cache.getByText("60%")).toBeInTheDocument();
    expect(cache.getByText("Cache informado em 2 de 10 chamadas.")).toBeInTheDocument();
    expect(screen.getByText("90% menos conteúdo nesses resultados.")).toBeInTheDocument();
    const localReads = within(screen.getByRole("region", { name: "Releituras locais" }));
    expect(localReads.getByText("97,5% menos conteúdo nas releituras.")).toBeInTheDocument();
    expect(localReads.getByText("4")).toBeInTheDocument();
    expect(localReads.getByText(/confirmar que o conteúdo não mudou/)).toBeInTheDocument();
    const details = screen.getByRole("button", { name: "Ver medições detalhadas" });
    expect(details).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Orientações para evitar repetição")).not.toBeInTheDocument();
    await user.click(details);
    expect(screen.getByText("Consultas ao conteúdo completo").nextSibling).toHaveTextContent("7");
    expect(screen.getByText("Orientações para evitar repetição").nextSibling).toHaveTextContent("2");
    expect(screen.getByText("Chamadas repetidas evitadas").nextSibling).toHaveTextContent("1");
    expect(screen.getByText(/Visão e busca já estão incluídas/)).toBeVisible();
    expect(screen.getByText(/Não representam uma estimativa de dinheiro ou tempo/)).toBeVisible();
  });
  it("distinguishes an explicit zero hit from missing reporting", () => {
    render(<UsageEfficiency metrics={{ ...projectMetrics().metrics,
      efficiency: { ...emptyEfficiency, cacheReadRequests: 1, cacheReadInputTokens: 1000 } }} />);
    expect(within(screen.getByRole("region", { name: "Cache do provedor" })).getByText("0%")).toBeInTheDocument();
    expect(screen.queryByText("Sem medição")).not.toBeInTheDocument();
  });
  it("does not display a cache percentage when reported calls contain no input tokens", () => {
    render(<UsageEfficiency metrics={{ ...projectMetrics().metrics,
      efficiency: { ...emptyEfficiency, cacheReadRequests: 1 } }} />);
    expect(screen.getByText("Sem medição")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });
});
