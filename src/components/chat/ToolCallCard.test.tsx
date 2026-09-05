import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ToolCallCard } from "./ToolCallCard";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));

describe("ToolCallCard Web Search", () => {
  it("mostra a consulta compacta e expande conta, resposta e fontes clicáveis", async () => {
    const user = userEvent.setup();
    render(<ToolCallCard tool={{ id: "search1", name: "web_search", status: "completed", args: { query: "Tauri documentação" }, output: JSON.stringify({ accountAlias: "openai-codex-pesquisa", model: "gpt-5.4", answer: "Resultado verificado", sources: [{ title: "Documentação oficial", url: "https://v2.tauri.app/" }] }) }} />);
    expect(screen.queryByText("Resultado verificado")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Pesquisa na web · Tauri documentação/ }));
    expect(screen.getByText("openai-codex-pesquisa · gpt-5.4")).toBeInTheDocument();
    expect(await screen.findByText("Resultado verificado")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Documentação oficial" }));
    expect(openUrl).toHaveBeenCalledWith("https://v2.tauri.app/");
  });
});
