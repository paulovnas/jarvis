import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ToolCallCard } from "./ToolCallCard";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));

describe("ToolCallCard Web Search", () => {
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
