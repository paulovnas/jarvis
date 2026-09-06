import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ValidationBatch } from "@/core/workflow";
import { WorkflowValidation } from "./WorkflowValidation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const batch: ValidationBatch = { id: "round", runId: "run", flow: "complete", epicIds: ["epic"], submitted: false, stale: false, createdAt: 1, items: [
  { id: "open", title: "Abrir projeto", steps: ["Selecione o projeto na sidebar."], expected: "O dashboard aparece.", decision: "pending", reason: null },
  { id: "save", title: "Salvar projeto", steps: ["Altere o nome e salve."], expected: "O novo nome aparece.", decision: "pending", reason: null },
] };
beforeEach(() => vi.mocked(invoke).mockReset().mockResolvedValue(undefined));
describe("Manual workflow validation", () => {
  it("approves, requires a rejection reason and submits only after every item is reviewed", async () => {
    const user = userEvent.setup(); const refresh = vi.fn();
    render(<WorkflowValidation conversationId="chat" batch={batch} busy={false} onRefresh={refresh} />);
    expect(screen.queryByRole("button", { name: "Encaminhar resultado" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Abrir projeto: pendente" }));
    expect(screen.getByText("Selecione o projeto na sidebar.")).toBeVisible();
    expect(screen.getByText("O dashboard aparece.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Aprovar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(within(screen.getByRole("button", { name: "Abrir projeto: aprovado" })).getByText("Abrir projeto")).toHaveClass("line-through");
    expect(screen.queryByRole("button", { name: "Encaminhar resultado" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Salvar projeto: pendente" }));
    await user.click(screen.getByRole("button", { name: "Reprovar" }));
    expect(screen.getByRole("button", { name: "Confirmar reprovação" })).toBeDisabled();
    await user.type(screen.getByRole("textbox", { name: "O que não funcionou?" }), "O nome voltou ao anterior.");
    await user.click(screen.getByRole("button", { name: "Confirmar reprovação" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Salvar projeto: reprovado" })).toHaveClass("text-destructive");
    expect(invoke).toHaveBeenCalledWith("decide_workflow_validation", { conversationId: "chat", batchId: "round", itemId: "save", decision: "rejected", reason: "O nome voltou ao anterior." });
    await user.click(screen.getByRole("button", { name: "Encaminhar resultado" }));
    expect(invoke).toHaveBeenCalledWith("submit_workflow_validation", { conversationId: "chat", batchId: "round" });
    expect(screen.getByText("Encaminhado ao Planejador")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Encaminhar resultado" })).not.toBeInTheDocument();
  });
  it("keeps a failed decision editable and disables decisions while the workflow runs", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockRejectedValueOnce({ message: "Disco indisponível" });
    const view = render(<WorkflowValidation conversationId="chat" batch={batch} busy={false} onRefresh={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "Abrir projeto: pendente" }));
    await user.click(screen.getByRole("button", { name: "Aprovar" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Aprovar" })).toBeEnabled());
    expect(screen.getByRole("dialog")).toBeVisible();
    view.rerender(<WorkflowValidation conversationId="chat" batch={batch} busy onRefresh={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Aprovar" })).toBeDisabled();
  });
  it("restores recorded decisions and prevents resubmitting stale or submitted rounds", async () => {
    const reviewed = { ...batch, items: batch.items.map(item => ({ ...item, decision: "approved" as const })) };
    const view = render(<WorkflowValidation conversationId="chat" batch={reviewed} busy={false} onRefresh={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Abrir projeto: aprovado" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Encaminhar resultado" })).toBeEnabled();
    view.rerender(<WorkflowValidation conversationId="chat" batch={{ ...reviewed, stale: true }} busy={false} onRefresh={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "Encaminhar resultado" })).not.toBeInTheDocument();
    view.rerender(<WorkflowValidation conversationId="chat" batch={{ ...reviewed, submitted: true }} busy={false} onRefresh={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "Encaminhar resultado" })).not.toBeInTheDocument();
  });
});
