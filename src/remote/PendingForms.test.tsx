import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import type { PendingAuthoring } from "@/core/authoring";
import type { RemoteChat } from "./client";
import { PendingForms, type RemoteAction } from "./PendingForms";

const emptyBundle = (): RemoteChat => ({ chat: emptyChat(), workflow: null, options: null });
const action = () => vi.fn<RemoteAction>().mockResolvedValue(true);

it("renders the root decision once and routes a separate subagent decision to its owner", async () => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle();
  const question = { turnId: "t1", toolId: "q1", questions: [{ id: "detail", question: "Qual detalhe?", options: [] }] };
  bundle.chat.pendingQuestion = question;
  const root = { id: "main", parentId: null, role: "planner" as const, title: "Planejador", status: "waiting" as const, createdAt: 1, updatedAt: 1, startedAt: 1, durationMs: 0, currentThought: null, attempts: 1, options: savedTurn().options, beadId: null, handoff: null, error: null, activeTurnId: "t1", pendingApproval: null, pendingQuestion: question };
  bundle.workflow = { conversationId: "c1", revision: 1, flow: "planned", agents: [root, { ...root, id: "designer1", parentId: "main", role: "designer", title: "Designer", pendingQuestion: { ...question, turnId: "sub1", toolId: "subq1", questions: [{ id: "color", question: "Qual cor?", options: [] }] } }] };
  render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} />);
  expect(screen.getAllByText("Qual detalhe?")).toHaveLength(1);
  expect(screen.getAllByRole("region", { name: "Perguntas do Jarvis" })).toHaveLength(2);
  await user.type(screen.getAllByRole("textbox", { name: "Sua resposta" })[1], "Azul");
  await user.click(screen.getAllByRole("button", { name: "Enviar respostas" })[1]);
  expect(onAction).toHaveBeenCalledWith("question", { conversationId: "c1", agentId: "designer1", turnId: "sub1", toolId: "subq1", response: { cancelled: false, answers: [{ id: "color", value: "Azul" }] } });
});

it("submits both an option and typed text to the original question IDs", async () => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle();
  bundle.chat.pendingQuestion = { turnId: "t1", toolId: "q1", questions: [{ id: "choice", question: "Qual caminho?", options: [{ label: "Plano simples", description: "Reusar o que existe" }] }, { id: "detail", question: "Qual detalhe?", options: [] }] };
  render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} />);
  await user.click(screen.getByRole("button", { name: /Plano simples/ }));
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await user.type(screen.getByRole("textbox", { name: "Sua resposta" }), "Conservar o rascunho");
  await user.click(screen.getByRole("button", { name: "Enviar respostas" }));
  expect(onAction).toHaveBeenCalledWith("question", { conversationId: "c1", agentId: "main", turnId: "t1", toolId: "q1", response: { cancelled: false, answers: [{ id: "choice", value: "Plano simples", selectedLabel: "Plano simples" }, { id: "detail", value: "Conservar o rascunho" }] } });
});

it.each([true, false])("sends tool authorization %s with its original turn/tool identity", async approved => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle();
  bundle.chat.activeTurnId = "t1";
  bundle.chat.pendingApproval = { tool: { id: "tool1", name: "bash", args: { command: "git status" }, status: "pending", output: "", durationMs: 0 }, policy: null };
  render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} />);
  expect(screen.getByText("git status")).toBeVisible();
  await user.click(screen.getByRole("button", { name: approved ? "Autorizar uma vez" : "Recusar" }));
  expect(onAction).toHaveBeenCalledWith("approval", { conversationId: "c1", agentId: "main", turnId: "t1", toolId: "tool1", decision: { approved, grant: null } });
});

const proposal = (): PendingAuthoring => ({ turnId: "t1", toolId: "github1", action: "publish", summary: "Publicar correção", catalogRevision: null, agentReferences: [], target: { kind: "publication", after: { summary: "Publicar correção", authorization: null, repositories: [{ path: ".", files: ["src/main.ts"], reset: null, branch: "codex/mobile", commitMessage: "fix: mobile", sync: "none", push: "normal", pullRequest: { base: "main", title: "Correção móvel", body: "Descrição completa do PR", draft: true, merge: { method: "squash", deleteBranch: true } } }] } } });

it.each([true, false])("shows complete GitHub operations before approval %s", async approved => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle(); bundle.chat.pendingAuthoring = proposal();
  render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} />);
  expect(screen.getByText("src/main.ts")).toBeVisible(); expect(screen.getByText("Descrição completa do PR")).toBeVisible(); expect(screen.getByText(/Excluir branch/)).toBeVisible();
  await user.click(screen.getByRole("button", { name: approved ? "Aprovar e executar" : "Recusar" }));
  expect(onAction).toHaveBeenCalledWith("validation", { conversationId: "c1", agentId: "main", kind: "publication", decision: { turnId: "t1", toolId: "github1", approved, note: null } });
});

it("requests publication revision when the user supplies a note", async () => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle(); bundle.chat.pendingAuthoring = proposal();
  render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} />);
  await user.type(screen.getByRole("textbox", { name: "Orientação para o agente (opcional)" }), "Manter a branch");
  expect(screen.queryByRole("button", { name: "Aprovar e executar" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Enviar para revisão" }));
  expect(onAction).toHaveBeenCalledWith("validation", expect.objectContaining({ decision: { turnId: "t1", toolId: "github1", approved: true, note: "Manter a branch" } }));
});

it("keeps requested operations visible in focused review and expands complete file and PR details before approval", async () => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle(); const request = proposal();
  if (request.target.kind !== "publication") throw new Error("Missing publication fixture");
  request.target.after.repositories[0] = { ...request.target.after.repositories[0], reset: { mode: "soft", target: "HEAD~1" }, sync: "rebase", syncBase: "origin/hml", push: "force_with_lease" };
  bundle.chat.pendingAuthoring = request;
  render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} focused />);
  expect(screen.getByRole("region", { name: "Aprovação Git e GitHub" })).toBeVisible();
  expect(screen.getByText("git reset --soft HEAD~1")).toBeVisible(); expect(screen.getByText("codex/mobile")).toBeVisible();
  expect(screen.getByText("fix: mobile")).toBeVisible(); expect(screen.getByText("rebase")).toBeVisible(); expect(screen.getByText("origin/hml")).toBeVisible();
  expect(screen.getByText("Push com force-with-lease")).toBeVisible(); expect(screen.getByText("Base: main")).toBeVisible();
  expect(screen.getByText("Correção móvel")).toBeVisible(); expect(screen.getByText(/Pull request.*Rascunho/)).toBeVisible(); expect(screen.getByText(/squash.*Excluir branch/)).toBeVisible();
  expect(screen.queryByText("src/main.ts")).not.toBeInTheDocument(); expect(screen.queryByText("Descrição completa do PR")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Arquivos · 1" }));
  await user.click(screen.getByRole("button", { name: "Descrição do PR" }));
  expect(screen.getByText("src/main.ts")).toBeVisible(); expect(screen.getByText("Descrição completa do PR")).toBeVisible();
  expect(onAction).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Aprovar e executar" }));
  expect(onAction).toHaveBeenCalledExactlyOnceWith("validation", { conversationId: "c1", agentId: "main", kind: "publication", decision: { turnId: "t1", toolId: "github1", approved: true, note: null } });
});

it.each([true, false])("preserves the focused approval draft when returning from the conversation and explicitly submits decision %s", async approved => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle(); const formDrafts = new Map<string, string>();
  bundle.chat.pendingAuthoring = proposal();
  const view = render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} formDrafts={formDrafts} focused />);
  await user.type(screen.getByRole("textbox", { name: "Orientação para o agente (opcional)" }), "Manter a branch");
  view.unmount();
  render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} formDrafts={formDrafts} focused />);
  expect(screen.getByRole("textbox", { name: "Orientação para o agente (opcional)" })).toHaveValue("Manter a branch");
  expect(screen.queryByRole("button", { name: "Aprovar e executar" })).not.toBeInTheDocument(); expect(onAction).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: approved ? "Enviar para revisão" : "Recusar" }));
  expect(onAction).toHaveBeenCalledExactlyOnceWith("validation", { conversationId: "c1", agentId: "main", kind: "publication", decision: { turnId: "t1", toolId: "github1", approved, note: "Manter a branch" } });
});

it("requires the manual rejection reason and reviews every item before submitting", async () => {
  const user = userEvent.setup(); const onAction = action(); const bundle = emptyBundle();
  bundle.workflow = { conversationId: "c1", revision: 1, flow: "complete", agents: [], validation: { id: "b1", runId: "run1", flow: "complete", epicIds: [], createdAt: 1, submitted: false, stale: false, items: [{ id: "i1", title: "Testar login", steps: ["Abrir o app", "Entrar"], expected: "Chat aberto", decision: "pending", reason: null }] } };
  const { rerender } = render(<PendingForms bundle={bundle} projectPath="/projeto" busy={false} onAction={onAction} />);
  expect(screen.getByText("Abrir o app")).toBeVisible(); expect(screen.getByText("Chat aberto")).toBeVisible();
  expect(screen.getByRole("button", { name: "Reprovar item" })).toBeDisabled(); expect(screen.getByRole("button", { name: "Encaminhar resultado" })).toBeDisabled();
  await user.type(screen.getByRole("textbox", { name: /O que não funcionou/ }), "A tela ficou vazia");
  await user.click(screen.getByRole("button", { name: "Reprovar item" }));
  expect(onAction).toHaveBeenCalledWith("validation", { conversationId: "c1", kind: "item", batchId: "b1", itemId: "i1", decision: "rejected", reason: "A tela ficou vazia" });
  const validation = bundle.workflow.validation;
  if (!validation) throw new Error("Missing validation fixture");
  rerender(<PendingForms bundle={{ ...bundle, workflow: { ...bundle.workflow, validation: { ...validation, items: validation.items.map(item => ({ ...item, decision: "rejected" })) } } }} projectPath="/projeto" busy={false} onAction={onAction} />);
  await user.click(screen.getByRole("button", { name: "Encaminhar resultado" }));
  expect(onAction).toHaveBeenLastCalledWith("validation", { conversationId: "c1", kind: "submit", batchId: "b1" });
});
