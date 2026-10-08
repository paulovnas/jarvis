import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { PendingAuthoring } from "@/core/authoring";

import { AuthoringApprovalDrawer } from "./AuthoringApprovalDrawer";

const agent = {
  id: "a".repeat(32),
  name: "Especialista em acessibilidade",
  description: "Revisa interfaces e entrega evidências WCAG.",
  instructions: "## Objetivo\n\nRevise a interface e liste evidências verificáveis.",
  usage: "mixed" as const,
  capability: "read_only" as const,
  deniedTools: ["bash"],
  model: null,
  appearance: { icon: "shield" as const, color: "cyan" as const },
};

function agentRequest(): PendingAuthoring {
  return {
    turnId: "turn-1", toolId: "tool-1", action: "create", catalogRevision: 4,
    summary: "Adicionar um agente especializado em acessibilidade.",
    target: { kind: "agent", before: null, after: agent }, agentReferences: [],
  };
}

it("compares project instructions and waits for an explicit approval", async () => {
  const user = userEvent.setup(); const answer = vi.fn().mockResolvedValue(true);
  const before = "# Existing rules\nUse bun.\n";
  const after = `${before}\n<!-- BEGIN JARVIS PROJECT INSTRUCTIONS -->\nRun bun run check.\n<!-- END JARVIS PROJECT INSTRUCTIONS -->\n`;
  const request: PendingAuthoring = { turnId: "instructions-turn", toolId: "instructions-tool", action: "update", catalogRevision: null, summary: "Registrar a verificação do projeto", agentReferences: [], target: { kind: "project_instructions", path: "AGENTS.md", before, after } };
  render(<AuthoringApprovalDrawer request={request} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar instruções do projeto" });
  expect(within(dialog).getByLabelText("Conteúdo proposto do AGENTS.md")).toHaveValue(after);
  await user.click(within(dialog).getByRole("tab", { name: "Arquivo atual" }));
  expect(within(dialog).getByLabelText("Conteúdo atual do AGENTS.md")).toHaveValue(before);
  const overlay = document.querySelector('[data-slot="sheet-overlay"]');
  if (!(overlay instanceof HTMLElement)) throw new Error("Missing approval overlay");
  await user.click(overlay); await user.keyboard("{Escape}");
  expect(answer).not.toHaveBeenCalled(); expect(dialog).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Aprovar e salvar" }));
  expect(answer).toHaveBeenCalledExactlyOnceWith(true, null);
});

it("reviews an updated plugin with exact commands and preserves approval on outside clicks", async () => {
  const user = userEvent.setup(); const answer = vi.fn().mockResolvedValue(true);
  const command = 'node "${PLUGIN_ROOT}/hooks/check.mjs" --validate';
  const request: PendingAuthoring = { turnId: "plugin-turn", toolId: "plugin-update", action: "update", catalogRevision: 8, summary: "Atualizar o plugin de produção", agentReferences: [], target: { kind: "plugin", preview: { title: "Atualizar produção", description: "Revisar o pacote antes de atualizar", source: "https://github.com/org/production.git", hash: "sha256:verified-package", components: [{ id: "hooks:start", name: "Preparar produção", kind: "hooks", enabled: true, supported: true, trusted: false, detail: "Executa antes da tarefa." }], commands: [command], requirements: ["Node.js 22"], warnings: ["O core nativo já oferece este recurso."], affectedIds: ["production@community"] } } };
  render(<AuthoringApprovalDrawer request={request} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByText(command)).toBeVisible();
  expect(within(dialog).getByText("Node.js 22")).toBeVisible();
  expect(within(dialog).getByText("O core nativo já oferece este recurso.")).toBeVisible();
  const approve = within(dialog).getByRole("button", { name: "Aprovar e salvar" });
  expect(approve).toBeEnabled();
  const overlay = document.querySelector('[data-slot="sheet-overlay"]');
  if (!(overlay instanceof HTMLElement)) throw new Error("Missing approval overlay");
  await user.click(overlay); await user.keyboard("{Escape}");
  expect(answer).not.toHaveBeenCalled(); expect(dialog).toBeVisible(); expect(approve).toBeEnabled();
  await user.click(approve); expect(answer).toHaveBeenCalledExactlyOnceWith(true, null);
});

function mcpRequest(transport: "stdio" | "http" = "stdio"): PendingAuthoring {
  return {
    turnId: "turn-mcp", toolId: "tool-mcp", action: "create", catalogRevision: null,
    summary: "Adicionar um servidor de documentação ao Jarvis.", agentReferences: [],
    target: { kind: "mcp", server: {
      name: "documentacao", transport, enabled: transport === "stdio",
      command: transport === "stdio" ? "node" : null,
      args: transport === "stdio" ? ["/docs/servidor MCP.js", "--modo", "consulta"] : [],
      url: transport === "http" ? "https://docs.example.com/mcp" : null,
      cwd: transport === "stdio" ? "/projetos/documentacao" : null,
      envKeys: transport === "stdio" ? ["API_KEY", "REGION"] : [],
      headerKeys: transport === "http" ? ["Authorization", "X-Workspace"] : [],
    } },
  };
}

it("reviews exact MCP process scope without connecting before explicit approval", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  render(<AuthoringApprovalDrawer request={mcpRequest()} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar servidor MCP" });
  expect(within(dialog).getByText("documentacao")).toBeVisible();
  expect(within(dialog).getByText("Local · stdio")).toBeVisible();
  expect(within(dialog).getByText("Global · disponível nos projetos e chats do Jarvis")).toBeVisible();
  expect(within(dialog).getByText("node")).toBeVisible();
  expect(within(dialog).getByText('["/docs/servidor MCP.js","--modo","consulta"]')).toBeVisible();
  expect(within(dialog).getByText("/projetos/documentacao")).toBeVisible();
  expect(within(dialog).getByText("API_KEY")).toBeVisible();
  expect(within(dialog).getByText("REGION")).toBeVisible();
  expect(within(dialog).getByText("Ativado")).toBeVisible();
  expect(within(dialog).getByText(/iniciar o processo.*após sua aprovação/)).toBeVisible();
  const approval = within(dialog).getByRole("button", { name: "Aprovar e adicionar" });
  const key = within(dialog).getByLabelText("Variável API_KEY");
  expect(key).toHaveAttribute("type", "password");
  expect(approval).toBeDisabled();
  await user.type(key, "   ");
  expect(approval).toBeDisabled();
  await user.clear(key);
  await user.type(key, "local-test-secret");
  expect(approval).toBeDisabled();
  await user.type(within(dialog).getByLabelText("Variável REGION"), "global");
  expect(approval).toBeEnabled();
  expect(within(dialog).queryByText("local-test-secret")).not.toBeInTheDocument();
  expect(answer).not.toHaveBeenCalled();
  await user.click(approval);
  expect(answer).toHaveBeenCalledWith(true, null, { environment: { API_KEY: "local-test-secret", REGION: "global" }, headers: {} });
});

it("reviews an inactive remote MCP with header names and preserves it on outside clicks and Escape", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  render(<AuthoringApprovalDrawer request={mcpRequest("http")} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar servidor MCP" });
  expect(within(dialog).getByText("Remoto · HTTP")).toBeVisible();
  expect(within(dialog).getByText("https://docs.example.com/mcp")).toBeVisible();
  expect(within(dialog).getByText("Authorization")).toBeVisible();
  expect(within(dialog).getByText("X-Workspace")).toBeVisible();
  expect(within(dialog).getByText("Desativado")).toBeVisible();
  expect(within(dialog).getByText(/permanecerá desativado.*sem iniciar processos ou conexões/)).toBeVisible();
  expect(within(dialog).queryByText("Programa")).not.toBeInTheDocument();
  await user.type(within(dialog).getByLabelText("Cabeçalho Authorization"), "Bearer test-secret");
  await user.type(within(dialog).getByLabelText(/Orientação para o agente/), "Use o servidor de homologação");
  const overlay = document.querySelector('[data-slot="sheet-overlay"]');
  expect(overlay).toBeInstanceOf(HTMLElement);
  await user.click(overlay as HTMLElement);
  await user.keyboard("{Escape}");
  expect(answer).not.toHaveBeenCalled();
  expect(dialog).toBeVisible();
  expect(within(dialog).getByLabelText(/Orientação para o agente/)).toHaveValue("Use o servidor de homologação");
  expect(within(dialog).getByLabelText("Cabeçalho Authorization")).toHaveValue("Bearer test-secret");
  await user.click(within(dialog).getByRole("button", { name: "Recusar" }));
  expect(answer).toHaveBeenCalledWith(false, "Use o servidor de homologação");
});

it("sends remote MCP credentials only with explicit approval and keeps the pending preview free of values", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  const request = mcpRequest("http");
  const original = JSON.stringify(request);
  render(<AuthoringApprovalDrawer request={request} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar servidor MCP" });
  const approval = within(dialog).getByRole("button", { name: "Aprovar e adicionar" });
  expect(approval).toBeDisabled();
  await user.type(within(dialog).getByLabelText("Cabeçalho Authorization"), "Bearer remote-test-secret");
  expect(approval).toBeDisabled();
  await user.type(within(dialog).getByLabelText("Cabeçalho X-Workspace"), "test-workspace");
  expect(approval).toBeEnabled();
  expect(within(dialog).queryByText("Bearer remote-test-secret")).not.toBeInTheDocument();
  expect(JSON.stringify(request)).toBe(original);
  expect(answer).not.toHaveBeenCalled();
  await user.click(approval);
  expect(answer).toHaveBeenCalledWith(true, null, { environment: {}, headers: { Authorization: "Bearer remote-test-secret", "X-Workspace": "test-workspace" } });
});

it("reviews an agent proposal and only saves after explicit approval", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  render(<AuthoringApprovalDrawer request={agentRequest()} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("heading", { name: "Revisar alteração no Jarvis" })).toBeVisible();
  expect(within(dialog).getByText("Especialista em acessibilidade")).toBeVisible();
  expect(await within(dialog).findByRole("heading", { name: "Objetivo" })).toBeVisible();
  expect(within(dialog).getByText("Herdar do chat")).toBeVisible();
  expect(within(dialog).getByText("Misto")).toBeVisible();
  expect(answer).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole("button", { name: "Aprovar e salvar" }));
  expect(answer).toHaveBeenCalledWith(true, null);
});

it("shows flow routing with readable agent names and returns a rejection note", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  const first = "b".repeat(32); const second = "c".repeat(32);
  const before = {
    id: "d".repeat(32), name: "Fluxo de revisão", description: "Revisa entregas.",
    entry: first, maxSteps: 2, appearance: { icon: "route" as const, color: "purple" as const },
    steps: [{ id: first, agentId: agent.id, instructions: "Revisar", position: { x: 10, y: 10 }, next: null, onRework: null }],
  };
  const request: PendingAuthoring = {
    turnId: "turn-2", toolId: "tool-2", action: "update", catalogRevision: 7,
    summary: "Adicionar uma etapa de validação ao fluxo.",
    target: { kind: "flow", before, after: { ...before, maxSteps: 4, steps: [
      { ...before.steps[0], next: second },
      { id: second, agentId: agent.id, instructions: "Validar", position: { x: 320, y: 10 }, next: null, onRework: first },
    ] } },
    agentReferences: [{ id: agent.id, name: agent.name }],
  };
  render(<AuthoringApprovalDrawer request={request} owner="Planejador" onAnswer={answer} />);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getAllByText(agent.name)).toHaveLength(2);
  expect(within(dialog).getByText("Etapas")).toBeVisible();
  expect(within(dialog).getByText("Limite")).toBeVisible();
  await user.type(within(dialog).getByLabelText(/Orientação para o agente/), "Troque o nome antes de salvar");
  await user.click(within(dialog).getByRole("button", { name: "Recusar" }));
  expect(answer).toHaveBeenCalledWith(false, "Troque o nome antes de salvar");
});

it("shows the exact commit, PR and merge scope before publishing", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  const request: PendingAuthoring = {
    turnId: "turn-3", toolId: "tool-3", action: "publish", catalogRevision: null,
    summary: "Publicar frontend e backend em propostas separadas.",
    agentReferences: [],
    target: { kind: "publication", after: { summary: "Publicar frontend e backend em propostas separadas.", authorization: null, repositories: [
      { path: "frontend", reset: null, files: ["src/App.tsx"], branch: "feat/new-home", commitMessage: "feat(home): improve hero", sync: "none", push: "normal", pullRequest: { base: "main", title: "Melhora a página inicial", body: "## Alterações\n\nAtualiza a hero.", draft: false, merge: { method: "squash", deleteBranch: true } } },
      { path: "backend", reset: null, files: ["src/server.ts"], branch: null, commitMessage: "fix(api): validate request", sync: "none", push: "none", pullRequest: null },
    ] } },
  };
  render(<AuthoringApprovalDrawer request={request} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar ações Git e GitHub" });
  expect(within(dialog).getByText("frontend")).toBeVisible();
  expect(within(dialog).getByText("backend")).toBeVisible();
  expect(within(dialog).getByText("feat(home): improve hero")).toBeVisible();
  expect(within(dialog).getByText("src/App.tsx")).toBeVisible();
  expect(await within(dialog).findByRole("heading", { name: "Alterações" })).toBeVisible();
  expect(within(dialog).getByText("Push para origin")).toBeVisible();
  expect(within(dialog).getByText(/Merge após localizar ou criar · squash/)).toBeVisible();
  expect(answer).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole("button", { name: "Aprovar e executar" }));
  expect(answer).toHaveBeenCalledWith(true, null);
});

it("preserves pending publication and its observation on outside clicks and Escape until explicit revision", async () => {
  const user = userEvent.setup();
  const answer = vi.fn().mockResolvedValue(true);
  const request: PendingAuthoring = {
    turnId: "turn-4", toolId: "tool-4", action: "publish", catalogRevision: null,
    summary: "Publicar somente os arquivos aprovados.", agentReferences: [],
    target: { kind: "publication", after: { summary: "Publicar somente os arquivos aprovados.", authorization: null, repositories: [
      { path: ".", reset: null, files: ["src/App.tsx", "docs/picpay.ofx"], branch: null, commitMessage: "fix: adjust publication", sync: "none", push: "normal", pullRequest: null },
    ] } },
  };
  render(<AuthoringApprovalDrawer request={request} onAnswer={answer} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar ações Git e GitHub" });

  await user.type(within(dialog).getByLabelText(/Orientação para o agente/), "Ignore docs/picpay.ofx");
  expect(within(dialog).getByText(/nenhuma ação será executada agora/i)).toBeVisible();
  const overlay = document.querySelector('[data-slot="sheet-overlay"]');
  expect(overlay).toBeInstanceOf(HTMLElement);
  await user.click(overlay as HTMLElement);
  expect(answer).not.toHaveBeenCalled();
  expect(dialog).toBeVisible();
  await user.keyboard("{Escape}");

  expect(answer).not.toHaveBeenCalled();
  expect(dialog).toBeVisible();
  expect(within(dialog).getByLabelText(/Orientação para o agente/)).toHaveValue("Ignore docs/picpay.ofx");
  await user.click(within(dialog).getByRole("button", { name: "Enviar para revisão" }));

  expect(answer).toHaveBeenCalledWith(true, "Ignore docs/picpay.ofx");
  expect(answer).toHaveBeenCalledTimes(1);
});

it("shows a supervised soft reset without requiring a commit", () => {
  const request: PendingAuthoring = {
    turnId: "turn-4", toolId: "tool-4", action: "publish", catalogRevision: null,
    summary: "Desfazer o último commit e manter as alterações preparadas.",
    agentReferences: [],
    target: { kind: "publication", after: { summary: "Desfazer o último commit e manter as alterações preparadas.", authorization: null, repositories: [
      { path: "movart-express-back", reset: { mode: "soft", target: "HEAD^" }, files: [], branch: null, commitMessage: null, sync: "none", push: "none", pullRequest: null },
    ] } },
  };
  render(<AuthoringApprovalDrawer request={request} onAnswer={vi.fn().mockResolvedValue(true)} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar ações Git e GitHub" });
  expect(within(dialog).getByText("Reorganizar histórico")).toBeVisible();
  expect(within(dialog).getByText("git reset --soft HEAD^")).toBeVisible();
  expect(within(dialog).queryByText("Commit")).not.toBeInTheDocument();
});

it("shows local remote synchronization separately from push", () => {
  const request: PendingAuthoring = {
    turnId: "turn-sync", toolId: "tool-sync", action: "publish", catalogRevision: null,
    summary: "Atualizar hml local com origin/hml.", agentReferences: [],
    target: { kind: "publication", after: { summary: "Atualizar hml local com origin/hml.", authorization: null, repositories: [
      { path: "portal", reset: null, files: [], branch: "hml", commitMessage: null, sync: "rebase", push: "none", pullRequest: null },
    ] } },
  };

  render(<AuthoringApprovalDrawer request={request} onAnswer={vi.fn().mockResolvedValue(true)} />);
  const dialog = screen.getByRole("dialog", { name: "Revisar ações Git e GitHub" });
  expect(within(dialog).getByText("Atualizar branch local com origin")).toBeVisible();
  expect(within(dialog).getByText(/reaplica commits locais/)).toBeVisible();
  expect(within(dialog).queryByText("Push para origin")).not.toBeInTheDocument();
});

it("discloses requested Fast and consumption before saving an authored agent", () => {
  const request = agentRequest();
  if (request.target.kind !== "agent") throw new Error("Expected agent request");
  request.target.after = { ...request.target.after, model: { account: "work", model: "sol", reasoning: "high", serviceTier: "priority" } };
  render(<AuthoringApprovalDrawer request={request} onAnswer={vi.fn()} />);
  expect(screen.getByText("work / sol · Fast / high")).toBeVisible();
  expect(screen.getByText("Maior consumo dos limites/créditos")).toBeVisible();
});
