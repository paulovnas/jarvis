import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { Skill, SkillSnapshot } from "@/core/skills";
import { SkillsSettings } from "./SkillsSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const mocked = vi.mocked(invoke);
const own: Skill = { id: "own", name: "react-expert", description: "Build React interfaces", origin: "jarvis", path: "/home/.jarvis/skills/react", enabled: true, automatic: true, source: null, marketplaceId: null, updateAvailable: false, updateError: null };
let state: SkillSnapshot;

describe("SkillsSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    state = { includeAgents: false, directory: "/home/.jarvis/skills", skills: [own], warnings: [] };
    mocked.mockReset().mockImplementation(async (command, args) => {
      if (command === "list_skills" || command === "check_skill_updates") return state;
      if (command === "delete_skill") { state = { ...state, skills: state.skills.filter(skill => skill.id !== (args as { id: string }).id) }; return state; }
      if (command === "set_skills_agents") { state = { ...state, includeAgents: true, skills: [own, { ...own, id: "shared", name: "shared-skill", origin: "agents" }] }; return state; }
      if (command === "set_skill_enabled") { const values = args as { id: string; enabled: boolean }; state = { ...state, skills: state.skills.map(skill => skill.id === values.id ? { ...skill, enabled: values.enabled } : skill) }; return state; }
      if (command === "get_skill_detail") return { name: own.name, description: own.description, content: "---\nname: react-expert\n---\n# React workflow\nRead existing components first.", path: own.path, source: null, files: ["SKILL.md"] };
      if (command === "update_skills") { const values = args as { ids: string[] }; state = { ...state, skills: state.skills.map(skill => values.ids.includes(skill.id) ? { ...skill, updateAvailable: false } : skill) }; return { snapshot: state, updated: values.ids.length, errors: [] }; }
      throw new Error(`Unexpected command: ${command}`);
    });
  });
  it("mostra a pasta do Jarvis por padrão e inclui .agents pelo switch", async () => {
    const user = userEvent.setup(); const count = vi.fn(); render(<SkillsSettings onCountChange={count} />);
    await screen.findByRole("button", { name: "Detalhes de react-expert" });
    expect(screen.getByRole("switch", { name: "Incluir .agents/skills" })).not.toBeChecked();
    expect(screen.getByText("/home/.jarvis/skills")).toBeVisible();
    await user.click(screen.getByRole("switch", { name: "Incluir .agents/skills" }));
    expect(await screen.findByRole("button", { name: "Detalhes de shared-skill" })).toBeVisible();
    expect(mocked).toHaveBeenCalledWith("set_skills_agents", { enabled: true });
    expect(count).toHaveBeenLastCalledWith(2);
  });
  it("abre instruções na modal e desativa sem remover a skill", async () => {
    const user = userEvent.setup(); render(<SkillsSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes de react-expert" }));
    const dialog = screen.getByRole("dialog", { name: own.name });
    expect(await within(dialog).findByText("Read existing components first.")).toBeVisible();
    expect(within(dialog).queryByText("name: react-expert")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await user.click(screen.getByRole("switch", { name: "Ativar react-expert" }));
    await waitFor(() => expect(screen.getByRole("switch", { name: "Ativar react-expert" })).not.toBeChecked());
    expect(screen.getByText("Inativa")).toBeVisible();
    expect(mocked).toHaveBeenCalledWith("set_skill_enabled", { id: "own", enabled: false });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("verifica atualizações automaticamente e permite atualizar todas", async () => {
    state = { ...state, skills: [{ ...own, source: "a/b", marketplaceId: "a/b/react", updateAvailable: true }, { ...own, id: "two", name: "rust", source: "a/b", marketplaceId: "a/b/rust", updateAvailable: true }] };
    const user = userEvent.setup(); render(<SkillsSettings />);
    const button = await screen.findByRole("button", { name: /Atualizar todas/ });
    await waitFor(() => expect(button).toBeEnabled());
    expect(mocked).toHaveBeenCalledWith("check_skill_updates");
    await user.click(button);
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("update_skills", { ids: ["own", "two"] }));
    expect(await screen.findByRole("button", { name: "Detalhes de react-expert" })).toBeVisible();
    await waitFor(() => expect(screen.queryByRole("button", { name: /Atualizar todas/ })).not.toBeInTheDocument());
    expect(toast.success).toHaveBeenCalledWith("2 skills atualizadas");
  });
  it("oferece atualização individual e filtra a lista", async () => {
    state = { ...state, skills: [{ ...own, source: "a/b", marketplaceId: "a/b/react", updateAvailable: true }] };
    const user = userEvent.setup(); render(<SkillsSettings />);
    const button = await screen.findByRole("button", { name: "Atualizar react-expert" });
    await waitFor(() => expect(button).toBeEnabled());
    expect(screen.queryByRole("button", { name: /Atualizar todas/ })).not.toBeInTheDocument();
    await user.click(button);
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("update_skills", { ids: ["own"] }));
    fireEvent.change(screen.getByRole("textbox", { name: "Buscar skills instaladas" }), { target: { value: "not-found" } });
    expect(screen.getByText("Nenhuma skill encontrada.")).toBeVisible();
  });
  it("mantém o estado anterior se o armazenamento falhar", async () => {
    const user = userEvent.setup(); render(<SkillsSettings />);
    await screen.findByRole("button", { name: "Detalhes de react-expert" });
    mocked.mockRejectedValueOnce({ code: "skill_error", message: "Falha ao salvar" });
    await user.click(screen.getByRole("switch", { name: "Ativar react-expert" }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Falha ao salvar"));
    expect(screen.getByRole("switch", { name: "Ativar react-expert" })).toBeChecked();
  });
  it("confirma exclusão, permite cancelar e atualiza a contagem após excluir", async () => {
    const user = userEvent.setup(); const count = vi.fn(); render(<SkillsSettings onCountChange={count} />);
    await user.click(await screen.findByRole("button", { name: "Excluir react-expert" }));
    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText(/excluídos definitivamente/)).toBeVisible();
    expect(mocked).not.toHaveBeenCalledWith("delete_skill", expect.anything());
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(screen.getByRole("button", { name: "Detalhes de react-expert" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Excluir react-expert" }));
    await user.click(screen.getByRole("button", { name: "Excluir skill" }));
    expect(await screen.findByText("Nenhuma skill instalada.")).toBeVisible();
    expect(mocked).toHaveBeenCalledWith("delete_skill", { id: "own" });
    expect(count).toHaveBeenLastCalledWith(0);
  });
  it("informa exclusão compartilhada e mantém a modal e a skill se falhar", async () => {
    state = { ...state, skills: [{ ...own, origin: "agents" }] };
    const user = userEvent.setup(); render(<SkillsSettings />);
    await user.click(await screen.findByRole("button", { name: "Excluir react-expert" }));
    expect(screen.getByText(/afetando também outros agentes/)).toBeVisible();
    mocked.mockRejectedValueOnce({ code: "skill_error", message: "Pasta protegida" });
    await user.click(screen.getByRole("button", { name: "Excluir skill" }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Pasta protegida"));
    expect(screen.getByRole("alertdialog")).toBeVisible();
  });
});
