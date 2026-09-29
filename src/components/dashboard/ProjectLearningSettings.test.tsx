import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { markdownSource } from "@/test/markdown-editor";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { LearningSnapshot } from "@/core/project-learning";
import { ProjectLearningSettings } from "./ProjectLearningSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const call = vi.mocked(invoke);
let snapshot: LearningSnapshot;
let failSave: boolean;
beforeEach(() => {
  failSave = false;
  snapshot = { enabled: true, revision: 0, pending: 0, notice: null, lessons: [{
    id: "lesson", scope: ".", content: "Exiba o label no Select do projeto.", topics: ["select"], check: "Verificar o texto visível.",
    status: "active", origin: "feedback", evidence: [{ conversationId: "chat-a", messageId: "message-a", excerpt: "Sempre exiba o label, não o value", createdAt: 1000 }], revision: 1, updatedAt: 1000,
  }] };
  call.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_project_learning") return structuredClone(snapshot);
    if (command === "set_project_learning") { snapshot.enabled = false; snapshot.revision += 1; return structuredClone(snapshot); }
    if (command === "save_project_lesson") {
      if (failSave) throw { message: "Este aprendizado mudou. Seu rascunho foi preservado." };
      const request = args as { lesson: Partial<LearningSnapshot["lessons"][number]> };
      snapshot.lessons[0] = { ...snapshot.lessons[0], ...request.lesson, revision: 2, origin: "user" }; return structuredClone(snapshot);
    }
    if (command === "delete_project_lesson") { snapshot.lessons = []; return structuredClone(snapshot); }
    if (command === "preview_project_learning_import") return { version: 1, lessons: [{ scope: "frontend", content: "Use o Select", topics: ["select"], check: "" }] };
    if (command === "import_project_learning") return structuredClone(snapshot);
    if (command === "export_project_learning") return true;
    if (command === "get_project_knowledge") return { scopes: ["."], documents: [{ scope: ".", kind: "rules", path: "rules.md", content: "# Regras existentes", essential: "Respeite o escopo", revision: "original", sources: [], staleSources: [], error: null }] };
    if (command === "save_project_knowledge") return {};
    throw new Error(`Unexpected command ${command}`);
  });
});

it("loads scoped lessons, displays their source and disables learning without deleting records", async () => {
  const user = userEvent.setup(); render(<ProjectLearningSettings projectId="p1" />);
  expect(screen.getByRole("status", { name: "Carregando aprendizados" })).toBeVisible();
  await user.click(await screen.findByRole("button", { name: /Exiba o label/ }));
  expect(screen.getByText("Sempre exiba o label, não o value")).toBeVisible();
  await user.click(screen.getByRole("switch", { name: "Aprender com minhas correções" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("set_project_learning", { projectId: "p1", enabled: false, revision: 0 }));
  expect(screen.getByText("Exiba o label no Select do projeto.")).toBeVisible();
  expect(screen.getByRole("switch")).not.toBeChecked();
});

it("shows the selected status label and saves edits with their optimistic revision", async () => {
  const user = userEvent.setup(); render(<ProjectLearningSettings projectId="p1" />);
  await user.click(await screen.findByRole("button", { name: /Exiba o label/ }));
  await user.click(screen.getByRole("button", { name: "Editar" }));
  expect(screen.getByRole("combobox", { name: "Uso pelos agentes" })).toHaveTextContent("Ativo");
  await user.click(screen.getByRole("combobox", { name: "Uso pelos agentes" }));
  await user.click(await screen.findByRole("option", { name: "Desativado" }));
  const editor = screen.getByRole("textbox", { name: "Lição" });
  await user.clear(editor); await user.type(editor, "Preserve o label após carregar as opções.");
  await user.click(screen.getByRole("button", { name: "Salvar aprendizado" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_lesson", { projectId: "p1", lesson: expect.objectContaining({ id: "lesson", revision: 1, status: "disabled", content: "Preserve o label após carregar as opções." }) }));
});

it("preserves a conflicting draft and only deletes after the explicit UI action", async () => {
  const user = userEvent.setup(); render(<ProjectLearningSettings projectId="p1" />);
  await user.click(await screen.findByRole("button", { name: /Exiba o label/ }));
  await user.click(screen.getByRole("button", { name: "Editar" }));
  const editor = screen.getByRole("textbox", { name: "Lição" }); await user.type(editor, " Nova edição."); failSave = true;
  await user.click(screen.getByRole("button", { name: "Salvar aprendizado" }));
  await waitFor(() => expect(within(screen.getByRole("dialog")).getByRole("alert")).toHaveTextContent("rascunho foi preservado"));
  expect(editor).toHaveValue("Exiba o label no Select do projeto. Nova edição.");
  await user.click(screen.getByRole("button", { name: "Cancelar" }));
  await user.click(screen.getByRole("button", { name: "Excluir" }));
  expect(call).not.toHaveBeenCalledWith("delete_project_lesson", expect.anything());
  await user.click(screen.getByRole("button", { name: "Excluir aprendizado" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("delete_project_lesson", { projectId: "p1", id: "lesson", revision: 1 }));
});

it("previews imports and promotion without silently rewriting maintained rules", async () => {
  const user = userEvent.setup(); render(<ProjectLearningSettings projectId="p1" />);
  await user.click(await screen.findByRole("button", { name: "Importar" }));
  expect(await screen.findByRole("dialog", { name: "Revisar importação" })).toBeVisible();
  expect(call).not.toHaveBeenCalledWith("import_project_learning", expect.anything());
  await user.click(screen.getByRole("button", { name: "Importar sugestões" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("import_project_learning", { projectId: "p1", content: expect.stringContaining('"scope": "frontend"') }));
  await user.click(screen.getByRole("button", { name: /Exiba o label/ }));
  await user.click(screen.getByRole("button", { name: "Incorporar às regras" }));
  expect(await markdownSource(user, "Atual")).toHaveValue("# Regras existentes");
  expect(await markdownSource(user, "Após incorporar")).toHaveValue("# Regras existentes\n\n- Exiba o label no Select do projeto. Verificação: Verificar o texto visível.\n");
  expect(call).not.toHaveBeenCalledWith("save_project_knowledge", expect.anything());
  await user.click(screen.getByRole("button", { name: "Salvar regras" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("save_project_knowledge", { projectId: "p1", document: expect.objectContaining({ revision: "original", essential: "Respeite o escopo" }) }));
});
