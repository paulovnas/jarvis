import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatArea } from "./ChatArea";

describe("ChatArea component", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    window.HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  it("renderiza o cabeçalho do chat, mensagens do usuário e respostas do assistente", () => {
    render(<ChatArea />);

    expect(screen.getByRole("main", { name: "Área do chat" })).toBeInTheDocument();
    expect(screen.getByText("Sessão de Engenharia")).toBeInTheDocument();
    expect(screen.getByText(/Onboarding & App Shell/i)).toBeInTheDocument();

    // Mensagem do usuário com anexo
    expect(
      screen.getByText("Jarvis, implemente a persistência SQLite e o layout de três colunas para o desktop.")
    ).toBeInTheDocument();
    expect(screen.getByText("docs/PLAN-FEAT-onboarding-app-shell.md")).toBeInTheDocument();

    // Resposta do assistente
    expect(screen.getByText("Implementação concluída com sucesso")).toBeInTheDocument();
    expect(screen.getByText("Trabalhou por 14s")).toBeInTheDocument();
  });

  it("expande e recolhe o bloco 'Trabalhou por {tempo}' ao clicar", async () => {
    const user = userEvent.setup();
    render(<ChatArea />);

    // Inicialmente o detalhe de raciocínio não está visível
    expect(screen.queryByText("Raciocínio interno do agente")).not.toBeInTheDocument();

    // Clica no botão de colapso de trabalho
    const workButton = screen.getByRole("button", { name: /Trabalhou por 14s/i });
    await user.click(workButton);

    // Agora o raciocínio interno e as ferramentas executadas devem estar visíveis
    expect(screen.getByText("Raciocínio interno do agente")).toBeInTheDocument();
    expect(screen.getByText("Chamadas de ferramentas")).toBeInTheDocument();
    expect(screen.getByText("Leitura de arquivo")).toBeInTheDocument();
    expect(screen.getByText("Edição de código")).toBeInTheDocument();

    // Clica novamente para recolher
    await user.click(workButton);
    expect(screen.queryByText("Raciocínio interno do agente")).not.toBeInTheDocument();
  });

  it("expande uma chamada de ferramenta para visualizar parâmetros e saída", async () => {
    const user = userEvent.setup();
    render(<ChatArea />);

    // Abre o trabalho de 14s
    const workButton = screen.getByRole("button", { name: /Trabalhou por 14s/i });
    await user.click(workButton);

    // Clica na ferramenta "Leitura de arquivo"
    const toolButton = screen.getByRole("button", { name: /Leitura de arquivo/i });
    await user.click(toolButton);

    expect(screen.getByText("Saída do terminal")).toBeInTheDocument();
    expect(screen.getByText(/export const appConfig/i)).toBeInTheDocument();
  });

  it("renderiza o alerta de erro operacional e aciona o retry", async () => {
    const user = userEvent.setup();
    const toastInfoSpy = vi.spyOn(toast, "info");
    render(<ChatArea />);

    expect(
      screen.getByText("Servidor local de inteligência artificial offline")
    ).toBeInTheDocument();
    expect(screen.getByText(/\$ curl -s http:\/\/127.0.0.1:11434\/api\/tags/i)).toBeInTheDocument();

    const retryButton = screen.getByRole("button", { name: /Tentar novamente/i });
    await user.click(retryButton);

    expect(toastInfoSpy).toHaveBeenCalledWith("Ação mockada", expect.any(Object));
  });

  it("permite digitar e enviar uma nova mensagem pelo composer", async () => {
    const user = userEvent.setup();
    render(<ChatArea />);

    const textarea = screen.getByPlaceholderText(/Pergunte ou dê uma instrução ao Jarvis/i);
    await user.type(textarea, "Adicione validação para o novo layout");

    const sendButton = screen.getByRole("button", { name: "Enviar mensagem" });
    await user.click(sendButton);

    // A mensagem enviada deve aparecer no chat
    expect(screen.getByText("Adicione validação para o novo layout")).toBeInTheDocument();

    // Espera a resposta dinâmica do assistente
    await waitFor(
      () => {
        expect(
          screen.getByText(/Recebi sua solicitação: "Adicione validação para o novo layout"/i)
        ).toBeInTheDocument();
      },
      { timeout: 3000 }
    );
  });

  it("exibe os labels legíveis nos seletores de modelo e modo ao invés dos valores brutos", async () => {
    const user = userEvent.setup();
    render(<ChatArea />);

    const modelButton = screen.getByRole("button", {
      name: /Selecionar modelo de IA/i,
    });
    expect(modelButton).toHaveTextContent(/Gemini 2.5 Pro/i);
    expect(screen.queryByText("gemini-2.5-pro")).not.toBeInTheDocument();
    expect(screen.queryByText(/claude/i)).not.toBeInTheDocument();

    expect(
      screen.getByRole("button", { name: "Selecionar modo de execução" })
    ).toHaveTextContent("Build (Escrita & Execução)");
    expect(screen.queryByText("build")).not.toBeInTheDocument();

    // Clica no seletor de modelo para abrir o menu agrupado por provider
    await user.click(modelButton);
    await waitFor(() => {
      expect(screen.getByText("Antigravity")).toBeInTheDocument();
      expect(screen.getByText("OpenAI")).toBeInTheDocument();
    });
  });
});
