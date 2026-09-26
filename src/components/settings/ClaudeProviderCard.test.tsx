import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { ClaudeProviderPreferences } from "@/core/executors";
import { ExecutorModelPicker } from "@/components/chat/ExecutorModelPicker";
import { ClaudeProviderCard } from "./ClaudeProviderCard";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

it("configures one local provider and updates all model selectors after successful saves", async () => {
  const user = userEvent.setup();
  let preferences: ClaudeProviderPreferences = { enabled: true, disabledModels: [] };
  const model = (id: string) => ({ id, name: id.toUpperCase(), description: "CLI", reasoningLevels: [], defaultReasoning: null });
  const metadata = { installed: true, authenticated: true, version: "2.1", email: "person@example.test", authMethod: "oauth", subscriptionType: "max", error: null, models: [model("sonnet"), model("opus")] };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_claude_runtime" || command === "refresh_claude_runtime") return { ...metadata, preferences };
    if (command === "save_claude_provider_preferences") {
      preferences = (args as { preferences: ClaudeProviderPreferences }).preferences;
      return preferences;
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  render(<><ClaudeProviderCard /><ExecutorModelPicker modelGroups={[]} onSelect={vi.fn()} /></>);
  await waitFor(() => expect(screen.getByTestId("provider-account-claude-code")).toHaveTextContent("Conectado"));
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_claude_runtime")).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Detalhes de Claude Code" }));
  const dialog = screen.getByRole("dialog", { name: "Claude Code" });
  expect(dialog).toHaveTextContent("Provedor local único");
  expect(dialog).toHaveTextContent("Requer o CLI oficial instalado e autenticado");
  expect(dialog).toHaveTextContent("person@example.test");
  expect(dialog).toHaveTextContent("claude auth login");
  expect(within(dialog).queryByRole("textbox", { name: /Alias|API/i })).not.toBeInTheDocument();
  await user.click(screen.getByRole("switch", { name: "Disponibilizar SONNET" }));
  await waitFor(() => expect(screen.getByRole("switch", { name: "Disponibilizar SONNET" })).not.toBeChecked());
  expect(invoke).toHaveBeenCalledWith("save_claude_provider_preferences", { preferences: { enabled: true, disabledModels: ["sonnet"] } });
  metadata.models.push(model("haiku"));
  await user.click(screen.getByRole("button", { name: "Atualizar status e modelos" }));
  expect(await screen.findByRole("switch", { name: "Disponibilizar HAIKU" })).toBeChecked();
  expect(screen.getByRole("switch", { name: "Disponibilizar SONNET" })).not.toBeChecked();
  expect(screen.getByRole("switch", { name: "Mostrar limites do Claude Code" })).toBeChecked();
  await user.click(screen.getByRole("switch", { name: "Mostrar limites do Claude Code" }));
  await waitFor(() => expect(screen.getByRole("switch", { name: "Mostrar limites do Claude Code" })).not.toBeChecked());
  expect(preferences.showUsage).toBe(false);
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  expect(await screen.findByRole("menuitem", { name: "OPUS" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: "SONNET" })).not.toBeInTheDocument();
  await user.keyboard("{Escape}{Escape}");
  await user.click(screen.getByRole("button", { name: "Detalhes de Claude Code" }));
  vi.mocked(invoke).mockRejectedValueOnce(new Error("disk unavailable"));
  await user.click(screen.getByRole("switch", { name: "Ativar Claude Code" }));
  await waitFor(() => expect(toast.error).toHaveBeenCalled());
  expect(screen.getByRole("switch", { name: "Ativar Claude Code" })).toBeChecked();
  await user.click(screen.getByRole("switch", { name: "Ativar Claude Code" }));
  await waitFor(() => expect(screen.getByRole("switch", { name: "Ativar Claude Code" })).not.toBeChecked());
  await user.keyboard("{Escape}");
  expect(screen.getByTestId("provider-account-claude-code")).toHaveTextContent("Desativado");
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  expect(screen.queryByRole("menuitem", { name: "Claude Code" })).not.toBeInTheDocument();
});
