import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AppSidebar } from "./Sidebar";

describe("AppSidebar component", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renderiza o seletor de workspace, botão de novo workspace e nova conversa", () => {
    render(<AppSidebar />);

    expect(screen.getByRole("complementary", { name: "Workspace" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Workspace" })).toHaveTextContent("Pessoal");

    expect(screen.getByRole("button", { name: "Novo Workspace" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Nova conversa" })).toBeInTheDocument();
  });

  it("emite toast ao clicar em Novo Workspace e Nova Conversa", async () => {
    const user = userEvent.setup();
    const toastInfoSpy = vi.spyOn(toast, "info");
    const toastSuccessSpy = vi.spyOn(toast, "success");

    render(<AppSidebar />);

    await user.click(screen.getByRole("button", { name: "Novo Workspace" }));
    expect(toastInfoSpy).toHaveBeenCalledWith("Novo Workspace (mock)", expect.any(Object));

    await user.click(screen.getByRole("button", { name: "Nova conversa" }));
    expect(toastSuccessSpy).toHaveBeenCalledWith("Nova conversa iniciada!", expect.any(Object));
  });

  it("permite navegar pelos projetos e exibe as conversas aninhadas do projeto ativo", async () => {
    const user = userEvent.setup();
    render(<AppSidebar />);

    // Aba de projetos está aberta por padrão
    expect(screen.getByText("Jarvis")).toBeInTheDocument();
    expect(screen.getByText("Metis")).toBeInTheDocument();
    expect(screen.getByText("Projeto ativo")).toBeInTheDocument();

    // Conversas do Jarvis ativas aninhadas
    expect(screen.getByText("Onboarding app shell")).toBeInTheDocument();
    expect(screen.getByText("Persistência nativa")).toBeInTheDocument();

    // Clica no projeto Metis para ativá-lo
    await user.click(screen.getByText("Metis"));

    // Agora exibe as conversas do Metis
    expect(screen.getByText("Exploração de TUI")).toBeInTheDocument();
    expect(screen.getByText("Mapeamento de ferramentas")).toBeInTheDocument();
  });

  it("na aba de conversas contextualiza qual é o projeto pai das conversas", async () => {
    const user = userEvent.setup();
    render(<AppSidebar />);

    // Clica na aba Conversas
    const tab = screen.getByRole("tab", { name: /conversas/i });
    await user.click(tab);

    await waitFor(() => {
      expect(screen.getByText("Conversas do projeto")).toBeInTheDocument();
      expect(screen.getAllByText("Jarvis").length).toBeGreaterThanOrEqual(1);
      expect(
        screen.getByRole("button", { name: "Nova conversa no projeto" })
      ).toBeInTheDocument();
      expect(screen.getByText("Onboarding app shell")).toBeInTheDocument();
      expect(screen.getByText("Persistência nativa")).toBeInTheDocument();
    });
  });

  it("emite Sonner Em breve ao clicar em Configurações", async () => {
    const user = userEvent.setup();
    const toastInfoSpy = vi.spyOn(toast, "info");

    render(<AppSidebar />);
    await user.click(screen.getByRole("button", { name: /configurações/i }));

    expect(toastInfoSpy).toHaveBeenCalledWith("Em breve");
  });
});
