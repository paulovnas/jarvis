import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ExecutionSelection } from "@/core/executors";
import { KNOWLEDGE_KINDS, type KnowledgeDocument, type KnowledgeDraft, type KnowledgeKind } from "@/core/project-knowledge";
import { ProjectKnowledgeSettings } from "./ProjectKnowledgeSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("@/components/chat/ExecutorModelPicker", () => ({ ExecutorModelPicker: ({ onSelect }: { onSelect: (choice: ExecutionSelection) => void }) => <><button onClick={() => onSelect({ model: "account/model", reasoning: null })}>Modelo de teste</button><button onClick={() => onSelect({ executor: "claude", model: "sonnet", reasoning: null })}>Claude de teste</button></> }));

const call = vi.mocked(invoke);
const initial = (): KnowledgeDocument[] => [".", "frontend"].flatMap(scope => Object.keys(KNOWLEDGE_KINDS).map(kind => ({
  scope, kind: kind as KnowledgeKind, path: `${scope}/${KNOWLEDGE_KINDS[kind as KnowledgeKind].file}`,
  content: `# ${scope} ${kind}`, essential: "", revision: "original", sources: [], staleSources: [], error: null,
})));
let documents: KnowledgeDocument[];
let generate: () => Promise<KnowledgeDraft>;
let failSave: boolean;

beforeEach(() => {
  documents = initial(); failSave = false;
  generate = async () => ({ content: "# Produto gerado", revision: "original", sources: [{ path: "README.md", fingerprint: "hash" }] });
  call.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_project_knowledge") return { scopes: [".", "frontend"], documents };
    if (command === "generate_project_knowledge") return generate();
    if (command === "cancel_project_knowledge_generation") return;
    if (command === "save_project_knowledge") {
      if (failSave) throw { message: "O documento mudou. Recarregue antes de salvar." };
      const request = args as { document: KnowledgeDocument };
      return { ...documents.find(doc => doc.kind === request.document.kind && doc.scope === request.document.scope), ...request.document, revision: "saved" };
    }
    if (command === "link_project_knowledge") return { ...documents[0], path: "docs/produto.md", content: "# Fonte existente", revision: "linked" };
    if (command === "import_project_knowledge") return "# Importado";
    if (command === "save_markdown_document") return true;
    throw new Error(`Unexpected command ${command}`);
  });
});

it("preserves drafts across category and repository changes and saves only the selected scope", async () => {
  const user = userEvent.setup();
  render(<ProjectKnowledgeSettings projectId="p1" />);
  const editor = await screen.findByRole("textbox", { name: "Produto · Markdown" });
  await user.clear(editor); await user.type(editor, "# Produto editado");
  await user.click(screen.getByRole("tab", { name: "Regras" }));
  await user.type(screen.getByRole("textbox", { name: "Regras essenciais" }), "Preserve os dados.");
  await user.click(screen.getByRole("combobox", { name: "Escopo do conhecimento" }));
  await user.click(await screen.findByRole("option", { name: "frontend" }));
  expect(screen.getByRole("textbox", { name: "Regras essenciais" })).toHaveValue("");
  await user.type(screen.getByRole("textbox", { name: "Regras essenciais" }), "Use componentes existentes.");
  await user.click(screen.getByRole("button", { name: "Salvar conhecimento" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_knowledge", expect.objectContaining({ projectId: "p1", document: expect.objectContaining({ scope: "frontend", kind: "rules", essential: "Use componentes existentes." }) })));
  await user.click(screen.getByRole("combobox", { name: "Escopo do conhecimento" }));
  await user.click(await screen.findByRole("option", { name: "Projeto inteiro · compartilhado" }));
  expect(screen.getByRole("textbox", { name: "Regras essenciais" })).toHaveValue("Preserve os dados.");
  await user.click(screen.getByRole("tab", { name: "Produto" }));
  expect(screen.getByRole("textbox", { name: "Produto · Markdown" })).toHaveValue("# Produto editado");
});

it("generates a reviewable draft without overwriting edits or saving automatically", async () => {
  const user = userEvent.setup();
  render(<ProjectKnowledgeSettings projectId="p1" />);
  const editor = await screen.findByRole("textbox", { name: "Produto · Markdown" });
  expect(screen.getByText(/O processo pode demorar; você pode cancelar a geração a qualquer momento/)).toBeVisible();
  await user.type(editor, " manual");
  await user.click(screen.getByRole("button", { name: "Modelo de teste" }));
  await user.click(screen.getByRole("button", { name: "Analisar e gerar produto" }));
  expect(await screen.findByRole("dialog", { name: "Rascunho gerado" })).toBeVisible();
  expect(editor).toHaveValue("# . product manual");
  expect(call).not.toHaveBeenCalledWith("save_project_knowledge", expect.anything());
  await user.type(screen.getByRole("textbox", { name: "Prévia do rascunho" }), " revisado");
  await user.click(screen.getByRole("button", { name: "Usar no editor" }));
  expect(editor).toHaveValue("# Produto gerado revisado");
  await user.click(screen.getByRole("button", { name: "Salvar conhecimento" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_knowledge", expect.objectContaining({ document: expect.objectContaining({ content: "# Produto gerado revisado", sources: [{ path: "README.md", fingerprint: "hash" }] }) })));
});

it("cancels generation and ignores its late result, including for Claude", async () => {
  let resolve!: (draft: KnowledgeDraft) => void;
  generate = () => new Promise(done => { resolve = done; });
  const user = userEvent.setup();
  render(<ProjectKnowledgeSettings projectId="p1" />);
  const editor = await screen.findByRole("textbox", { name: "Produto · Markdown" });
  await user.click(screen.getByRole("button", { name: "Claude de teste" }));
  await user.click(screen.getByRole("button", { name: "Analisar e gerar produto" }));
  expect(call).toHaveBeenCalledWith("generate_project_knowledge", expect.objectContaining({ request: expect.objectContaining({ choice: { executor: "claude", account: "", model: "sonnet", reasoning: null } }) }));
  await user.type(editor, " nova edição");
  await user.click(screen.getByRole("button", { name: "Cancelar geração" }));
  await act(async () => resolve({ content: "resultado antigo", revision: "original", sources: [] }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(editor).toHaveValue("# . product nova edição");
  expect(call).toHaveBeenCalledWith("cancel_project_knowledge_generation", { id: expect.any(String) });
});

it("preserves edits after a revision conflict and requires an explicit choice to overwrite the new version", async () => {
  const user = userEvent.setup();
  render(<ProjectKnowledgeSettings projectId="p1" />);
  const editor = await screen.findByRole("textbox", { name: "Produto · Markdown" });
  await user.type(editor, " edição local");
  failSave = true;
  await user.click(screen.getByRole("button", { name: "Salvar conhecimento" }));
  expect(await screen.findByText("O documento mudou. Recarregue antes de salvar.")).toBeVisible();
  documents = documents.map(doc => doc.kind === "product" && doc.scope === "." ? { ...doc, content: "# Alteração externa", revision: "external" } : doc);
  await user.click(screen.getByRole("button", { name: "Recarregar documentos" }));
  expect(await screen.findByRole("button", { name: "Manter minhas edições sobre a versão atual" })).toBeVisible();
  expect(editor).toHaveValue("# . product edição local");
  expect(screen.getByRole("button", { name: "Salvar conhecimento" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Documento salvo e fontes" }));
  expect(screen.getByRole("textbox", { name: "Documento salvo" })).toHaveValue("# Alteração externa");
  await user.click(screen.getByRole("button", { name: "Manter minhas edições sobre a versão atual" }));
  failSave = false;
  await user.click(screen.getByRole("button", { name: "Salvar conhecimento" }));
  await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_project_knowledge", expect.objectContaining({ document: expect.objectContaining({ revision: "external", content: "# . product edição local" }) })));
});

it("links existing Markdown and imports/exports drafts without implicitly saving", async () => {
  const user = userEvent.setup();
  render(<ProjectKnowledgeSettings projectId="p1" />);
  const editor = await screen.findByRole("textbox", { name: "Produto · Markdown" });
  await user.click(screen.getByRole("button", { name: "Vincular existente" }));
  await user.type(screen.getByRole("textbox", { name: "Caminho relativo ao projeto" }), "docs/produto.md");
  await user.click(screen.getByRole("button", { name: "Vincular documento" }));
  await waitFor(() => expect(editor).toHaveValue("# Fonte existente"));
  await user.click(screen.getByRole("button", { name: "Importar MD" }));
  expect(await screen.findByRole("dialog", { name: "Markdown importado" })).toBeVisible();
  expect(editor).toHaveValue("# Fonte existente");
  await user.click(screen.getByRole("button", { name: "Usar no editor" }));
  await user.click(screen.getByRole("button", { name: "Exportar MD" }));
  expect(call).toHaveBeenCalledWith("save_markdown_document", { content: "# Importado", suggestedFileName: "prd.md" });
  expect(call).not.toHaveBeenCalledWith("save_project_knowledge", expect.anything());
});

it("keeps the editor usable after provider failure and cancels an unfinished job on unmount", async () => {
  const user = userEvent.setup();
  const view = render(<ProjectKnowledgeSettings projectId="p1" />);
  const editor = await screen.findByRole("textbox", { name: "Produto · Markdown" });
  generate = async () => { throw { message: "Provedor indisponível" }; };
  await user.click(screen.getByRole("button", { name: "Modelo de teste" }));
  await user.click(screen.getByRole("button", { name: "Analisar e gerar produto" }));
  expect(await screen.findByText("Provedor indisponível")).toBeVisible();
  expect(editor).toBeEnabled();
  generate = () => new Promise(() => {});
  await user.click(screen.getByRole("button", { name: "Analisar e gerar produto" }));
  view.unmount();
  expect(call).toHaveBeenCalledWith("cancel_project_knowledge_generation", { id: expect.any(String) });
});
