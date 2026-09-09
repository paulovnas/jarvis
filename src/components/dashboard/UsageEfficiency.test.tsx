import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { emptyEfficiency } from "@/core/dashboard";
import { projectMetrics } from "@/test/dashboard-fixtures";
import { UsageEfficiency } from "./UsageEfficiency";

describe("Usage efficiency", () => {
  it("does not invent cache hits for old sessions", () => {
    render(<UsageEfficiency metrics={projectMetrics().metrics} />);
    expect(within(screen.getByRole("region", { name: "Cache do provedor" })).getByText("Não informado")).toBeInTheDocument();
  });
  it("uses only measured input for cache percentage and shows partial coverage", () => {
    render(<UsageEfficiency metrics={{ ...projectMetrics().metrics, measuredSteps: 8, inputTokens: 5000,
      efficiency: { ...emptyEfficiency, cacheReadTokens: 600, cacheReadInputTokens: 1000, cacheReadRequests: 2,
        cacheWriteTokens: 200, cacheWriteRequests: 1, auxiliaryRequests: 2,
        indexedOutputs: 3, originalBytes: 10000, retainedBytes: 1000, contextSearches: 7,
        loopSteers: 2, loopAvoidedCalls: 1 } }} />);
    const cache = within(screen.getByRole("region", { name: "Cache do provedor" }));
    expect(cache.getByText("60%")).toBeInTheDocument();
    expect(cache.getByText("2/10 chamadas com leitura de cache informada")).toBeInTheDocument();
    expect(screen.getByText("90%")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Redução pelo Context-mode" })).getByText("7")).toBeInTheDocument();
    expect(screen.getByText("Loops orientados").nextSibling).toHaveTextContent("2");
    expect(screen.getByText("Repetições bloqueadas").nextSibling).toHaveTextContent("1");
    expect(screen.getByText("Incluídas nos tokens acumulados.")).toBeInTheDocument();
  });
  it("distinguishes an explicit zero hit from missing reporting", () => {
    render(<UsageEfficiency metrics={{ ...projectMetrics().metrics,
      efficiency: { ...emptyEfficiency, cacheReadRequests: 1, cacheReadInputTokens: 1000 } }} />);
    expect(within(screen.getByRole("region", { name: "Cache do provedor" })).getByText("0%")).toBeInTheDocument();
    expect(screen.queryByText("Não informado")).not.toBeInTheDocument();
  });
});
