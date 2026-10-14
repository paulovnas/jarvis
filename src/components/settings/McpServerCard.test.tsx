import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { McpServer } from "@/core/mcp";
import { McpServerCard } from "./McpServerCard";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const disconnected = { authenticated: false, state: "disconnected", error: null };
const connected = { authenticated: true, state: "connected", error: null };
const mocked = vi.mocked(invoke);
beforeEach(() => { vi.clearAllMocks(); mocked.mockReset().mockResolvedValue(disconnected); });

const server: McpServer = { id: "documentation", name: "Documentação", kind: "local", enabled: true, configured: true, revision: 1, lastCheck: { toolCount: 2, tools: ["query-docs", "resolve-library-id"], error: null } };

it("opens tools in a separate panel and restores keyboard focus on close", async () => {
  const user = userEvent.setup();
  render(<McpServerCard server={server} busy={false} onToggle={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} onTest={vi.fn()} />);
  const trigger = screen.getByRole("button", { name: "Detalhes do MCP Documentação" });
  expect(screen.queryByText("query-docs")).not.toBeInTheDocument();
  await user.click(trigger);
  const panel = screen.getByRole("dialog", { name: "Documentação" });
  expect(within(panel).getByText("query-docs")).toBeVisible();
  expect(trigger).not.toContainElement(panel);
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
});

it.each(["plugin-mcp:firebase", "plugin-app:firebase@official"])("identifies the owning plugin and prevents individual mutation for %s", async id => {
  const user = userEvent.setup();
  const onDelete = vi.fn();
  const onEdit = vi.fn();
  const onToggle = vi.fn();
  const onTest = vi.fn();
  render(<McpServerCard server={{ ...server, id, source: "firebase@official" }} busy={false} onToggle={onToggle} onEdit={onEdit} onDelete={onDelete} onTest={onTest} />);
  expect(screen.getByText("Plugin · firebase@official")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Detalhes do MCP Documentação" }));
  const panel = screen.getByRole("dialog", { name: "Documentação" });
  expect(within(panel).getByText("Gerenciado em Plugins")).toBeVisible();
  expect(within(panel).getByText(/Para remover este MCP, remova o plugin/)).toBeVisible();
  expect(within(panel).queryByRole("button", { name: "Excluir" })).not.toBeInTheDocument();
  expect(within(panel).queryByRole("button", { name: "Editar" })).not.toBeInTheDocument();
  expect(within(panel).queryByRole("switch")).not.toBeInTheDocument();
  await user.click(within(panel).getByRole("button", { name: "Testar conexão" }));
  expect(onTest).toHaveBeenCalledOnce();
  expect(onDelete).not.toHaveBeenCalled();
  expect(onEdit).not.toHaveBeenCalled();
  expect(onToggle).not.toHaveBeenCalled();
});

it.each([null, "jam@jam-plugins"])("offers explicit browser login in MCP details with source %s and refreshes its tools", async source => {
  const user = userEvent.setup(); const onTest = vi.fn();
  let finish!: (value: typeof connected) => void;
  const authorization = new Promise<typeof connected>(resolve => { finish = resolve; });
  mocked.mockImplementation(async command => command === "start_mcp_oauth" ? { flowId: "flow", authorizationUrl: "https://service.test/authorize" } : command === "wait_mcp_oauth" ? authorization : disconnected);
  render(<McpServerCard server={{ ...server, id: source ? "plugin-mcp:jam" : "jam", name: "Jam", kind: "remote", source }} busy={false} onToggle={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} onTest={onTest} />);
  await user.click(screen.getByRole("button", { name: "Detalhes do MCP Jam" }));
  const connect = await screen.findByRole("button", { name: "Conectar conta" });
  await waitFor(() => expect(connect).toBeEnabled());
  expect(openUrl).not.toHaveBeenCalled();
  await user.click(connect);
  expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://service.test/authorize");
  expect(screen.getByText("Aguardando navegador")).toBeVisible();
  expect(screen.getByRole("button", { name: "Cancelar conexão" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Testar conexão" })).toBeDisabled();
  await act(async () => finish(connected));
  expect(await screen.findByRole("button", { name: "Desconectar conta" })).toBeEnabled();
  expect(screen.getByText("Conectado")).toBeVisible();
  expect(onTest).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("button", { name: "Desconectar conta" }));
  expect(await screen.findByRole("button", { name: "Conectar conta" })).toBeEnabled();
  expect(onTest).toHaveBeenCalledTimes(2);
  expect(mocked).not.toHaveBeenCalledWith("delete_mcp_server", expect.anything());
});

it("preserves fixed-header and explicitly disabled OAuth connections without offering login", async () => {
  mocked.mockRejectedValue({ code: "mcp_oauth_configuration", message: "Este MCP não utiliza OAuth." });
  const user = userEvent.setup();
  render(<McpServerCard server={{ ...server, kind: "remote" }} busy={false} onToggle={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} onTest={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Detalhes do MCP Documentação" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("mcp_oauth_status", { id: server.id }));
  expect(screen.queryByRole("button", { name: "Conectar conta" })).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Testar conexão" })).toBeEnabled();
});

it("cancels manual browser login and unlocks the MCP controls for retry", async () => {
  const user = userEvent.setup();
  let finish!: (value: typeof disconnected) => void;
  const authorization = new Promise<typeof disconnected>(resolve => { finish = resolve; });
  mocked.mockImplementation(async command => command === "start_mcp_oauth" ? { flowId: "flow", authorizationUrl: "https://service.test/authorize" } : command === "wait_mcp_oauth" ? authorization : disconnected);
  render(<McpServerCard server={{ ...server, kind: "remote" }} busy={false} onToggle={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} onTest={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Detalhes do MCP Documentação" }));
  const connect = await screen.findByRole("button", { name: "Conectar conta" });
  await waitFor(() => expect(connect).toBeEnabled());
  await user.click(connect);
  expect(screen.getByRole("button", { name: "Excluir" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Cancelar conexão" }));
  expect(mocked).toHaveBeenCalledWith("cancel_mcp_oauth", { flowId: "flow" });
  await act(async () => finish(disconnected));
  expect(await screen.findByRole("button", { name: "Conectar conta" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Excluir" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Editar" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Testar conexão" })).toBeEnabled();
});
