import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ToolCallCard } from "./ToolCallCard";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("ToolCallCard Web Search", () => {
  it("loads deferred history details only when the action is expanded", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValue({ id: "read-large", name: "read", status: "completed", args: { path: "src/grande.ts" }, output: "conteúdo completo", durationMs: 18 });
    render(<ToolCallCard detailContext={{ conversationId: "chat-1", turnId: "turn-1" }} tool={{ id: "read-large", name: "read", status: "completed", args: { path: "src/grande.ts", _jarvisHistoryDetailsDeferred: true }, output: "", durationMs: 18 }} />);
    expect(invoke).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: /Leitura de arquivo.*src\/grande.ts/ }));
    expect(invoke).toHaveBeenCalledWith("get_chat_tool_call", { conversationId: "chat-1", turnId: "turn-1", toolId: "read-large" });
    expect(await screen.findByText("conteúdo completo")).toBeVisible();
    expect(screen.getByText(/"path": "src\/grande.ts"/)).toBeVisible();
  });

  it("identifies native direct-task updates", () => {
    render(<ToolCallCard tool={{ id: "tasks", name: "update_tasks", status: "completed", args: { tasks: [{ id: "build", title: "Implementar", status: "in_progress" }] }, output: "{\"updated\":1}" }} />);
    expect(screen.getByRole("button", { name: /Atualizar tarefas.*Concluída/ })).toBeVisible();
  });
  it("shows the direct-task preflight as attention instead of an execution failure", () => {
    const reminder = "Atualize a lista com update_tasks e mantenha uma tarefa em andamento antes de executar alterações.";
    render(<ToolCallCard tool={{ id: "task-reminder", name: "ctx_execute", status: "error", args: {}, output: reminder, error: reminder }} />);
    const trigger = screen.getByRole("button", { name: /Context Mode · Processamento.*Atenção/ });
    expect(trigger.querySelector(".text-onedark-yellow")).toBeInTheDocument();
    expect(trigger.querySelector(".text-destructive")).not.toBeInTheDocument();
  });
  it("distinguishes file mutations from blue read-only actions", () => {
    const { rerender } = render(<ToolCallCard tool={{ id: "write", name: "write", status: "completed", args: { path: "src/app.ts" }, output: "ok" }} />);
    expect(screen.getByRole("button", { name: /Escrita de arquivo/ }).querySelector(".text-onedark-yellow")).toBeInTheDocument();
    rerender(<ToolCallCard tool={{ id: "read", name: "read", status: "completed", args: { path: "src/app.ts" }, output: "ok" }} />);
    expect(screen.getByRole("button", { name: /Leitura de arquivo/ }).querySelector(".text-primary")).toBeInTheDocument();
  });
  it.each([
    ["apply_patch", "Patch transacional"],
    ["lsp_definition", "Código · Definição"],
    ["lsp_references", "Código · Referências"],
    ["lsp_symbols", "Código · Símbolos"],
    ["lsp_diagnostics", "Código · Diagnósticos"],
  ])("identifica a ferramenta nativa %s", (name, label) => {
    render(<ToolCallCard tool={{ id: name, name, status: "completed", args: { path: "src/app.ts" }, output: "{}" }} />);
    expect(screen.getByRole("button", { name: new RegExp(label) })).toBeVisible();
  });
  it("shows native Context7 queries compactly and expands their documentation", async () => {
    const user = userEvent.setup();
    render(<ToolCallCard tool={{ id: "docs", name: "context7_query_docs", status: "completed", args: { libraryId: "/websites/react_dev", query: "useState" }, output: "Documentação encontrada" }} />);
    expect(screen.queryByText("Documentação encontrada")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Documentação · Context7.*useState/ }));
    expect(await screen.findByText("Documentação encontrada")).toBeVisible();
  });
  it("expande a análise de imagem com o modelo exclusivo de Vision", async () => {
    const user = userEvent.setup();
    render(<ToolCallCard tool={{ id: "vision1", name: "vision", status: "completed", args: { question: "Qual a cor?" }, output: JSON.stringify({ accountAlias: "antigravity-pessoal", model: "gemini-3.8-flash", analysis: "O botão é azul." }) }} />);
    expect(screen.queryByText("O botão é azul.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Análise de imagem/ }));
    expect(await screen.findByText("O botão é azul.")).toBeVisible();
    expect(screen.getByText("antigravity-pessoal · gemini-3.8-flash")).toBeVisible();
  });
  it("identifica a delegação pelo título e mantém o despacho recolhido", async () => {
    const user = userEvent.setup();
    render(<ToolCallCard tool={{ id: "worker1", name: "hub_spawn", status: "completed", args: { title: "Revisar autenticação", prompt: "Inspecionar os critérios" }, output: "Agente iniciado" }} />);
    const trigger = screen.getByRole("button", { name: /Delegar tarefa.*Revisar autenticação/ });
    expect(screen.queryByText("Agente iniciado")).not.toBeInTheDocument();
    await user.click(trigger);
    expect(screen.getByText("Agente iniciado")).toBeVisible();
  });
  it("mantém o resultado do Beads recolhido e permite consultar a tarefa salva", async () => {
    const user = userEvent.setup();
    const output = JSON.stringify({ id: "project-task", title: "Corrigir seleção", status: "open" });
    render(<ToolCallCard tool={{ id: "beads1", name: "beads_create", status: "completed", args: { title: "Corrigir seleção" }, output }} />);
    expect(screen.queryByText(output)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /beads_create.*Concluída/ }));
    expect(screen.getByText(output)).toBeVisible();
  });
  it("mostra a skill pelo nome e mantém as instruções recolhidas", async () => {
    const user = userEvent.setup();
    render(<ToolCallCard tool={{ id: "skill1", name: "read_skill", status: "completed", args: { id: "opaque-id" }, output: "Skill: react-expert\nRead components first" }} />);
    const button = screen.getByRole("button", { name: /Leitura de skill.*react-expert/ });
    expect(screen.queryByText(/Read components first/)).not.toBeInTheDocument();
    await user.click(button);
    expect(screen.getByText(/Read components first/)).toBeVisible();
  });
  it("mostra a consulta compacta e expande conta, resposta e fontes clicáveis", async () => {
    const user = userEvent.setup();
    render(<ToolCallCard tool={{ id: "search1", name: "web_search", status: "completed", args: { query: "Tauri documentação" }, output: JSON.stringify({ accountAlias: "openai-codex-pesquisa", model: "gpt-5.4", answer: "Resultado verificado", sources: [{ title: "Documentação oficial", url: "https://v2.tauri.app/" }] }) }} />);
    expect(screen.queryByText("Resultado verificado")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Pesquisa na web.*Tauri documentação/ }));
    expect(screen.getByText("openai-codex-pesquisa · gpt-5.4")).toBeInTheDocument();
    expect(await screen.findByText("Resultado verificado")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Documentação oficial" }));
    expect(openUrl).toHaveBeenCalledWith("https://v2.tauri.app/");
  });
});
