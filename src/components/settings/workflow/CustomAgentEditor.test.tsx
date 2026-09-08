import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { customAgent } from "@/test/workflow-fixtures";
import { CustomAgentEditor } from "./CustomAgentEditor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const all = ["read_only", "write_files", "commands"];
beforeEach(() => vi.mocked(invoke).mockResolvedValue([
  { id: "web_search", name: "web_search", group: "Pesquisa", description: "Pesquisar na web.", required: false, capabilities: all },
  { id: "ask_user", name: "ask_user", group: "Conversa", description: "Solicitar respostas.", required: false, capabilities: all },
  { id: "ctx_search", name: "ctx_search", group: "Core", description: "Recuperar memória.", required: true, capabilities: all },
  { id: "bash", name: "bash", group: "Projeto", description: "Executar comandos.", required: false, capabilities: ["commands"] },
]));

it("saves individual tool permissions while preserving mandatory Core and capability limits", async () => {
  const user = userEvent.setup(); const save = vi.fn().mockResolvedValue(true);
  render(<CustomAgentEditor initial={customAgent} accounts={[]} saving={false} onSave={save} onClose={vi.fn()} creating />);
  await user.click(screen.getByRole("tab", { name: "Permissões" }));
  expect(screen.getByRole("tab", { name: "Permissões" })).toHaveAttribute("aria-selected", "true");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_agent_tool_permissions"));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(await screen.findByRole("switch", { name: "web_search" })).toBeChecked();
  expect(screen.getByRole("switch", { name: "ctx_search" })).toBeChecked();
  expect(screen.getByRole("switch", { name: "ctx_search" })).toHaveAttribute("aria-disabled", "true");
  expect(screen.getByRole("switch", { name: "bash" })).toHaveAttribute("aria-disabled", "true");
  await user.click(screen.getByRole("switch", { name: "web_search" }));
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  await waitFor(() => expect(save).toHaveBeenCalledWith({ ...customAgent, deniedTools: ["web_search"] }));
});

it("restores denied tools when editing and filters the catalog without discarding choices", async () => {
  const user = userEvent.setup(); const save = vi.fn().mockResolvedValue(true);
  render(<CustomAgentEditor initial={{ ...customAgent, deniedTools: ["ask_user"] }} accounts={[]} saving={false} onSave={save} onClose={vi.fn()} creating={false} />);
  await user.click(screen.getByRole("tab", { name: "Permissões" }));
  expect(await screen.findByRole("switch", { name: "ask_user" })).not.toBeChecked();
  await user.type(screen.getByPlaceholderText("Buscar ferramenta"), "web_search");
  expect(screen.queryByRole("switch", { name: "ask_user" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("switch", { name: "web_search" }));
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  await waitFor(() => expect(save).toHaveBeenCalledWith({ ...customAgent, deniedTools: ["ask_user", "web_search"] }));
});
