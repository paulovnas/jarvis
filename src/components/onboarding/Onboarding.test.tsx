import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { coreFixture } from "@/test/core-fixtures";
import { Onboarding } from "./Onboarding";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const optionalTools = {
  platform: "windows",
  platformLabel: "Windows",
  tools: [
    { id: "git", name: "Git", description: "Versionamento", installed: false, version: null, automaticInstall: true, installWith: "WinGet", helpUrl: "https://git-scm.com/download/win" },
    { id: "gh", name: "GitHub CLI", description: "Pull requests", installed: false, version: null, automaticInstall: true, installWith: "WinGet", helpUrl: "https://cli.github.com/" },
  ],
};
let changed: EventCallback<unknown> | undefined;
beforeEach(() => {
  invokeMock.mockReset(); changed = undefined;
  vi.mocked(listen).mockImplementation(async (name, callback) => { if (name === "core:changed") changed = callback; return () => {}; });
});
it("requires all six configured tools before providers and a connected provider before the final step", async () => {
  let state = coreFixture(); state.ready = false; state.items[4].configured = false;
  invokeMock.mockImplementation(async command => {
    if (command === "get_core_status" || command === "check_core_updates") return state;
    if (command === "get_optional_tools_status") return optionalTools;
    return [];
  });
  const user = userEvent.setup(); const complete = vi.fn();
  render(<Onboarding saving={false} onComplete={complete} />);
  expect(screen.getByRole("heading", { name: "Bem-vindo ao Jarvis" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await screen.findByText("5/6");
  expect(screen.getByRole("button", { name: "Avançar" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Configurar Context7" })).toBeEnabled();
  state = coreFixture();
  await act(async () => changed?.({ event: "core:changed", id: 1, payload: state }));
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
