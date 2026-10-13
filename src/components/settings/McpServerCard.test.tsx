import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { McpServer } from "@/core/mcp";
import { McpServerCard } from "./McpServerCard";

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
