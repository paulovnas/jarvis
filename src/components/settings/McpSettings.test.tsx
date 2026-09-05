import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MCP_TEMPLATE, type McpServer } from "@/core/mcp";
import { McpSettings } from "./McpSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const mocked = vi.mocked(invoke);
const server: McpServer = { id: "builtin-context7", name: "context7", kind: "local", enabled: true, configured: false, revision: 0, lastCheck: null };

describe("McpSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.mockReset().mockImplementation(async (command) => {
      if (command === "list_mcp_servers") return [server];
      if (command === "get_mcp_config") return MCP_TEMPLATE;
      throw new Error("Unexpected command");
    });
  });

  it("mantém o Context7 compacto e não conecta enquanto a chave é um exemplo", async () => {
    const user = userEvent.setup();
    render(<McpSettings />);
    const details = await screen.findByRole("button", { name: "Detalhes do MCP context7" });
    expect(details).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("Configuração pendente")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Testar conexão" })).not.toBeInTheDocument();
    await user.click(details);
    expect(screen.getByRole("button", { name: "Testar conexão" })).toBeDisabled();
    expect(mocked).toHaveBeenCalledExactlyOnceWith("list_mcp_servers");
  });

  it("abre o editor somente por ação do usuário, mostra ajuda e salva a chave", async () => {
    const user = userEvent.setup();
    render(<McpSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes do MCP context7" }));
    await user.click(screen.getByRole("button", { name: "Editar" }));
    const field = await screen.findByRole("textbox", { name: "Configuração JSON" });
    expect(field).toHaveValue(MCP_TEMPLATE);
    await user.click(screen.getByRole("button", { name: "Como configurar MCPs no OpenCode" }));
    expect(openUrl).toHaveBeenCalledWith("https://opencode.ai/docs/mcp-servers/");
    const configured = MCP_TEMPLATE.replace("YOUR_API_KEY", "test-only-key");
    fireEvent.change(field, { target: { value: configured } });
    mocked.mockResolvedValueOnce([{ ...server, configured: true, revision: 1 }]);
    await user.click(screen.getByRole("button", { name: "Salvar MCP" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mocked).toHaveBeenCalledWith("save_mcp_server", { id: server.id, config: configured });
    expect(screen.getByRole("button", { name: "Testar conexão" })).toBeEnabled();
    expect(screen.queryByDisplayValue(configured)).not.toBeInTheDocument();
    expect(mocked).not.toHaveBeenCalledWith("test_mcp_server", expect.anything());
  });

  it("valida o JSON e mantém o editor aberto após falha de armazenamento", async () => {
    const user = userEvent.setup();
    render(<McpSettings />);
    await screen.findByRole("button", { name: "Detalhes do MCP context7" });
    await user.click(screen.getByRole("button", { name: "Adicionar MCP" }));
    const field = screen.getByRole("textbox", { name: "Configuração JSON" });
    expect(field).toHaveAttribute("placeholder", MCP_TEMPLATE);
    fireEvent.change(field, { target: { value: "{" } });
    await user.click(screen.getByRole("button", { name: "Salvar MCP" }));
    expect(screen.getByRole("alert")).toHaveTextContent("JSON inválido");
    expect(mocked).toHaveBeenCalledTimes(1);
    fireEvent.change(field, { target: { value: MCP_TEMPLATE } });
    mocked.mockRejectedValueOnce({ code: "mcp_error", message: "Já existe um MCP com esse nome." });
    await user.click(screen.getByRole("button", { name: "Salvar MCP" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Já existe um MCP");
    expect(field).toHaveValue(MCP_TEMPLATE);
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("desativa o MCP sem excluir sua configuração e permite reativá-lo", async () => {
    const user = userEvent.setup();
    render(<McpSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes do MCP context7" }));
    mocked.mockResolvedValueOnce([{ ...server, enabled: false }]);
    await user.click(screen.getByRole("switch", { name: "Ativar MCP context7" }));
    await waitFor(() => expect(screen.getByRole("switch")).not.toBeChecked());
    expect(mocked).toHaveBeenLastCalledWith("set_mcp_enabled", { id: server.id, enabled: false });
    mocked.mockResolvedValueOnce([server]);
    await user.click(screen.getByRole("switch"));
    await waitFor(() => expect(screen.getByRole("switch")).toBeChecked());
    expect(mocked).toHaveBeenLastCalledWith("set_mcp_enabled", { id: server.id, enabled: true });
  });

  it("pede confirmação antes da exclusão e mantém os dados ao cancelar", async () => {
    const user = userEvent.setup();
    render(<McpSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes do MCP context7" }));
    await user.click(screen.getByRole("button", { name: "Excluir" }));
    let dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText(/removidas definitivamente/)).toBeVisible();
    expect(mocked).toHaveBeenCalledTimes(1);
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Excluir" }));
    dialog = screen.getByRole("alertdialog");
    mocked.mockResolvedValueOnce([]);
    await user.click(within(dialog).getByRole("button", { name: "Excluir MCP" }));
    expect(await screen.findByText(/Nenhum MCP cadastrado/)).toBeVisible();
    expect(mocked).toHaveBeenLastCalledWith("delete_mcp_server", { id: server.id });
  });

  it("testa a conexão somente quando solicitado e mostra as ferramentas descobertas", async () => {
    mocked.mockResolvedValueOnce([{ ...server, configured: true }]);
    const user = userEvent.setup();
    render(<McpSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes do MCP context7" }));
    mocked.mockResolvedValueOnce({ toolCount: 2, error: null });
    await user.click(screen.getByRole("button", { name: "Testar conexão" }));
    expect(await screen.findByText(/2 ferramentas na última verificação/)).toBeVisible();
    expect(mocked).toHaveBeenLastCalledWith("test_mcp_server", { id: server.id });
  });
});
