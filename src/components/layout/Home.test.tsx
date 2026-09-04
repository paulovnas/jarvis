import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Home from "./Home";

describe("Home shell", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renderiza a hierarquia desktop e os grupos estáticos do inspector", () => {
    render(<Home />);

    expect(screen.getByRole("complementary", { name: "Workspace" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Workspace" })).toHaveTextContent("Pessoal");
    expect(screen.getByRole("tab", { name: /projetos/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /conversas/i })).toBeInTheDocument();
    expect(screen.getByText("Projeto ativo")).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "Área do chat" })).toBeInTheDocument();
    expect(screen.getByText("Sessão de Engenharia")).toBeInTheDocument();
    expect(screen.getByText("Arquivos alterados")).toBeInTheDocument();
    expect(screen.getByText("Plano")).toBeInTheDocument();
    expect(screen.getByText("Subagentes")).toBeInTheDocument();
    expect(screen.getByText("Contexto")).toBeInTheDocument();
  });

  it("emite somente Sonner Em breve ao clicar em Configurações", async () => {
    const user = userEvent.setup();
    const toastInfoSpy = vi.spyOn(toast, "info");

    render(<Home />);
    await user.click(screen.getByRole("button", { name: "Configurações" }));

    expect(toastInfoSpy).toHaveBeenCalledTimes(1);
    expect(toastInfoSpy).toHaveBeenCalledWith("Em breve");
  });

});
