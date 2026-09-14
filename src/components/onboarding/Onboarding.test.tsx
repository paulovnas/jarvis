import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { coreFixture } from "@/test/core-fixtures";
import { Onboarding } from "./Onboarding";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const { backupOpenMock } = vi.hoisted(() => ({ backupOpenMock: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: backupOpenMock, save: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const optionalTools = {
  platform: "windows",
  platformLabel: "Windows",
  tools: [
    { id: "git", name: "Git", description: "Versionamento", installed: false, version: null, automaticInstall: true, installWith: "WinGet", helpUrl: "https://git-scm.com/download/win", checks: [] },
    { id: "gh", name: "GitHub CLI", description: "Pull requests", installed: false, version: null, automaticInstall: true, installWith: "WinGet", helpUrl: "https://cli.github.com/", checks: [] },
  ],
};
beforeEach(() => {
  invokeMock.mockReset(); backupOpenMock.mockReset();
  vi.mocked(listen).mockResolvedValue(() => {});
});
it("offers backup restoration before requiring Core or a provider", async () => {
  const state = coreFixture();
  backupOpenMock.mockResolvedValue("/tmp/restore.zip");
  invokeMock.mockImplementation(async command => {
    if (command === "get_core_status" || command === "check_core_updates") return state;
    if (command === "inspect_settings_backup") return {
      fingerprint: `sha256:${"a".repeat(64)}`,
      createdAt: 1,
      appVersion: "1.2.2",
      sourcePlatform: { id: "macos", label: "macOS" },
      archiveBytes: 1024,
      summary: { customAgents: 0, customFlows: 0, skills: 0, mcps: 0, modelTargets: 0 },
      modelTargets: [],
      warnings: [],
    };
    if (command === "import_settings_backup") return { summary: { customAgents: 0, customFlows: 0, skills: 0, mcps: 0, modelTargets: 0 }, mappedModels: 0 };
    return [];
  });
  const user = userEvent.setup();
  render(<Onboarding saving={false} onComplete={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Restaurar backup" }));
  const dialog = await screen.findByRole("dialog", { name: "Revisar backup" });
  await user.click(screen.getByRole("button", { name: "Restaurar configurações" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("import_settings_backup", expect.objectContaining({ path: "/tmp/restore.zip" })));
  expect(await screen.findByText("Backup restaurado. Conecte os provedores desta instalação para continuar.")).toBeVisible();
  await waitFor(() => expect(dialog).not.toBeInTheDocument());
});
it("requires essential Core tools but leaves Context7 optional before providers", async () => {
  const state = coreFixture(); state.items[4].configured = false;
  invokeMock.mockImplementation(async command => {
    if (command === "get_core_status" || command === "check_core_updates") return state;
    if (command === "get_optional_tools_status") return optionalTools;
    return [];
  });
  const user = userEvent.setup(); const complete = vi.fn();
  render(<Onboarding saving={false} onComplete={complete} />);
  expect(screen.getByRole("heading", { name: "Bem-vindo ao Jarvis" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await screen.findByText("5/5 essenciais");
  expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  expect(await screen.findByRole("heading", { name: "Complete seu ambiente" })).toBeVisible();
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await screen.findByRole("button", { name: "Adicionar conta" });
  expect(screen.getByRole("button", { name: "Avançar" })).toBeDisabled();
  expect(screen.queryByRole("region", { name: "Ferramentas" })).not.toBeInTheDocument();
  expect(complete).not.toHaveBeenCalled();
});
it("keeps tool settings visible after connection and sends the named workspace on completion", async () => {
  const accounts = [{ alias: "openai-codex-test", providerKind: "openai-codex", enabled: true, createdAt: 1, accountType: "personal", email: null, modelsAvailable: true, models: [{ id: "gpt-test", name: "Test", reasoningLevels: [], defaultReasoningLevel: null }] }];
  invokeMock.mockImplementation(async command => {
    if (command === "get_core_status" || command === "check_core_updates") return coreFixture();
    if (command === "get_optional_tools_status") return optionalTools;
    if (command === "list_provider_accounts") return accounts;
    if (command === "get_web_search_config" || command === "get_vision_config") return { accountAlias: null, model: null, inheritChat: true };
    return [];
  });
  const user = userEvent.setup(); const complete = vi.fn().mockResolvedValue(undefined);
  render(<Onboarding saving={false} onComplete={complete} />);
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await screen.findByRole("heading", { name: "Complete seu ambiente" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  expect(await screen.findByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("Herdar do chat");
  expect(await screen.findByRole("combobox", { name: "Provedor de Vision" })).toHaveTextContent("Herdar do chat");
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  expect(screen.getByLabelText("Workspace padrão")).toHaveAttribute("placeholder", "Pessoal");
  await user.type(screen.getByLabelText("Workspace padrão"), "Estúdio");
  await user.click(screen.getByRole("button", { name: "Começar" }));
  expect(complete).toHaveBeenCalledWith("Estúdio");
});
