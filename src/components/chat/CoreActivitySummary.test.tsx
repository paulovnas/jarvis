import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import type { CoreActivity } from "@/core/chat";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { CoreActivitySummary } from "./CoreActivitySummary";

const prepared: CoreActivity = {
  component: "open-design", action: "design_preparation", status: "applied",
  summary: "Identidade do projeto carregada", sources: ["project:DESIGN.md", "open-design:skills/design-brief"], durationMs: 3,
};
const step = { thinking: "", commentary: "Vou ajustar o layout.", tools: [], coreActivities: [prepared] };

it("shows real automatic work separately from model calls, with sources on demand", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ ...step, tools: [{ id: "d1", name: "design_read", status: "completed" }] }]} />);
  const trigger = screen.getByRole("button", { name: /Recursos do Core/ });
  expect(trigger).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByText("project:DESIGN.md")).not.toBeInTheDocument();
  await user.click(trigger);
  expect(screen.getByText("Automático")).toBeVisible();
  expect(screen.getByText(/Identidade do projeto carregada/)).toBeVisible();
  expect(screen.getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
  expect(screen.getByText("project:DESIGN.md")).toBeVisible();
});

it("keeps Core warnings live and moves the summary inside completed work", async () => {
  const user = userEvent.setup();
  const work = { durationSeconds: 12, steps: [step] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={work} />);
  expect(screen.getByText("Vou ajustar o layout.")).toBeVisible();
  const warning: CoreActivity = { ...prepared, component: "lsp", action: "post_mutation_diagnostics", status: "unavailable", summary: "O servidor não respondeu; os arquivos estão salvos.", sources: [] };
  const updated = { ...work, steps: [{ ...step, coreActivities: [prepared, warning] }] };
  rerender(<AssistantWorkCollapse isStreaming work={updated} />);
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText(/O servidor não respondeu; os arquivos estão salvos/)).toBeVisible();
  rerender(<AssistantWorkCollapse work={updated} />);
  expect(screen.queryByRole("button", { name: /Recursos do Core/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Trabalhou por 12s/ }));
  expect(screen.getByRole("button", { name: /Recursos do Core/ })).toHaveAttribute("aria-expanded", "false");
});

it("aggregates long runs without claiming missing or failed Core work succeeded", async () => {
  const user = userEvent.setup();
  const receipts = Array.from({ length: 100 }, () => ({ ...prepared, status: "reused" as const }));
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: receipts, tools: [{ id: "docs", name: "context7_query_docs", status: "error" }] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  const list = screen.getByRole("list", { name: "Uso dos recursos do Core" });
  expect(list.children).toHaveLength(2);
  expect(screen.getAllByText(/Identidade do projeto carregada/)).toHaveLength(1);
  const context7 = screen.getByText("Context7").closest("li")!;
  expect(within(context7).queryByText("Automático")).not.toBeInTheDocument();
  expect(within(context7).getByText(/0 chamadas concluídas · 1 não concluída/)).toBeVisible();
  expect(screen.queryByText("Ponytail")).not.toBeInTheDocument();
});

it("does not invent activity for legacy or empty histories", () => {
  const { container } = render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [] }]} />);
  expect(container).toBeEmptyDOMElement();
});
