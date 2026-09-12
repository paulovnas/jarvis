import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SkillsMarketplace } from "./SkillsMarketplace";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const mocked = vi.mocked(invoke);
const entries = Array.from({ length: 25 }, (_, index) => ({ id: `owner/repo/skill-${index}`, skillId: `skill-${index}`, name: `skill-${index}`, source: index === 0 ? "other/repo" : "owner/repo", installs: 100 + index }));
function mount(onInstalled = vi.fn()) { return render(<SkillsMarketplace open onOpenChange={vi.fn()} installed={[]} onInstalled={onInstalled} />); }
describe("SkillsMarketplace", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.mockReset().mockImplementation(async command => {
      if (command === "browse_skill_marketplace") return entries;
      if (command === "get_marketplace_skill") return { name: "skill-0", description: "Description", content: "# Workflow\nSkill instructions", path: null, source: "other/repo", files: ["SKILL.md"] };
      if (command === "install_marketplace_skill") return { includeAgents: false, directory: "/home/.jarvis/skills", skills: [], warnings: [] };
      throw new Error(`Unexpected command ${command}`);
    });
  });
  it("pagina o catálogo e oferece os três rankings", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("button", { name: "Ver skill-0" });
    expect(screen.queryByRole("button", { name: "Ver skill-24" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Próxima página" }));
    expect(screen.getByRole("button", { name: "Ver skill-24" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver skill-0" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Em alta" }));
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("browse_skill_marketplace", { query: "", ranking: "trending", limit: 60 }));
    expect(await screen.findByRole("button", { name: "Ver skill-0" })).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "Destaques" }));
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("browse_skill_marketplace", { query: "", ranking: "hot", limit: 60 }));
  });
  it("pesquisa com debounce e filtra por repositório", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("button", { name: "Ver skill-0" });
    fireEvent.change(screen.getByRole("textbox", { name: "Pesquisar no Marketplace" }), { target: { value: "react" } });
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("browse_skill_marketplace", { query: "react", ranking: "alltime", limit: 60 }));
    await screen.findByRole("button", { name: "Ver skill-0" });
    await user.click(screen.getByRole("combobox", { name: "Filtrar por repositório" }));
    await user.click(await screen.findByRole("option", { name: "other/repo" }));
    expect(screen.getByRole("button", { name: "Ver skill-0" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver skill-1" })).not.toBeInTheDocument();
  });
  it("mostra detalhes e instala apenas a skill escolhida", async () => {
    const user = userEvent.setup(); const installed = vi.fn(); mount(installed);
    await user.click(await screen.findByRole("button", { name: "Ver skill-0" }));
    const dialog = screen.getByRole("dialog", { name: "skill-0" });
    expect(await within(dialog).findByText("Skill instructions")).toBeVisible();
    expect(mocked).toHaveBeenCalledWith("get_marketplace_skill", { source: "other/repo", skillId: "skill-0" });
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await user.click(screen.getByRole("button", { name: "Instalar skill-0" }));
    await waitFor(() => expect(mocked).toHaveBeenCalledWith("install_marketplace_skill", { source: "other/repo", skillId: "skill-0" }));
    expect(installed).toHaveBeenCalledWith(expect.objectContaining({ directory: "/home/.jarvis/skills" }));
  });
  it("exibe falha do serviço e permite tentar novamente", async () => {
    mocked.mockRejectedValueOnce({ code: "skill_error", message: "Marketplace indisponível" });
    const user = userEvent.setup(); mount();
    expect(await screen.findByRole("alert")).toHaveTextContent("Marketplace indisponível");
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(await screen.findByRole("button", { name: "Ver skill-0" })).toBeVisible();
  });
  it("limita o combobox a 10 opções por padrão e filtra ao digitar", async () => {
    const manyRepos = Array.from({ length: 15 }, (_, i) => ({
      id: `org/repo-${i}/skill-${i}`,
      skillId: `skill-${i}`,
      name: `skill-${i}`,
      source: `org/repo-${i}`,
      installs: 10 + i,
    }));
    mocked.mockImplementation(async (command) => {
      if (command === "browse_skill_marketplace") return manyRepos;
      throw new Error(`Unexpected command ${command}`);
    });
    const user = userEvent.setup(); mount();
    await screen.findByRole("button", { name: "Ver skill-0" });

    await user.click(screen.getByRole("combobox", { name: "Filtrar por repositório" }));
    // As 10 primeiras opções devem estar visíveis (+ opção Todos os repositórios)
    expect(screen.getByRole("option", { name: "Todos os repositórios" })).toBeVisible();
    expect(screen.getByRole("option", { name: "org/repo-0" })).toBeVisible();
    expect(screen.getByRole("option", { name: "org/repo-9" })).toBeVisible();
    // A 11ª opção (repo-10 ou além) não deve estar na lista inicial dos 10 primeiros
    expect(screen.queryByRole("option", { name: "org/repo-14" })).not.toBeInTheDocument();

    // Digita no input de pesquisa do combobox
    const repoInput = screen.getByPlaceholderText("Pesquisar repositório...");
    await user.type(repoInput, "repo-14");

    // Agora deve encontrar org/repo-14 e não org/repo-0
    expect(await screen.findByRole("option", { name: "org/repo-14" })).toBeVisible();
    expect(screen.queryByRole("option", { name: "org/repo-0" })).not.toBeInTheDocument();

    // Seleciona org/repo-14
    await user.click(screen.getByRole("option", { name: "org/repo-14" }));
    expect(await screen.findByRole("button", { name: "Ver skill-14" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver skill-0" })).not.toBeInTheDocument();
  });
  it("filtra ao clicar na badge de repositório e abre link externo", async () => {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    const user = userEvent.setup(); mount();
    await screen.findByRole("button", { name: "Ver skill-0" });

    // Clica no link externo
    await user.click(screen.getByRole("button", { name: "Ver skill-0 no skills.sh" }));
    expect(openUrl).toHaveBeenCalledWith("https://skills.sh/other/repo/skill-0");

    // Clica na badge @other/repo no card
    await user.click(screen.getByRole("button", { name: "Filtrar por @other/repo" }));
    expect(screen.getByRole("button", { name: "Ver skill-0" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver skill-1" })).not.toBeInTheDocument();

    // Limpa o filtro de repositório clicando no badge de remoção
    await user.click(screen.getByRole("button", { name: "Limpar filtro de repositório" }));
    expect(screen.getByRole("button", { name: "Ver skill-1" })).toBeVisible();
  });
  it("limpa a barra de pesquisa ao clicar no botão X", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("button", { name: "Ver skill-0" });
    const searchInput = screen.getByRole("textbox", { name: "Pesquisar no Marketplace" });
    await user.type(searchInput, "react");
    expect(searchInput).toHaveValue("react");
    const clearBtn = screen.getByRole("button", { name: "Limpar pesquisa" });
    await user.click(clearBtn);
    expect(searchInput).toHaveValue("");
  });
});
