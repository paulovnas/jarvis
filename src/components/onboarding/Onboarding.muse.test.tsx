import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { expect, it, vi } from "vitest";
import { coreFixture } from "@/test/core-fixtures";
import { Onboarding } from "./Onboarding";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

it("starts with an authenticated Muse subscription without creating a native provider alias", async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === "get_core_status" || command === "check_core_updates") return coreFixture();
    if (command === "get_optional_tools_status") return { platform: "macos", platformLabel: "macOS", tools: [] };
    if (command === "get_muse_runtime" || command === "refresh_muse_runtime") return { installed: true, authenticated: true, version: "1", error: null, models: [{ id: "muse-wonder", name: "Muse Wonder", description: "", reasoningLevels: [], defaultReasoning: null }] };
    if (command === "get_claude_runtime") return { installed: false, authenticated: false, version: null, error: null, models: [] };
    return [];
  });
  const user = userEvent.setup(), complete = vi.fn().mockResolvedValue(undefined);
  render(<Onboarding saving={false} onComplete={complete} />);
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await screen.findByRole("heading", { name: "Complete seu ambiente" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await screen.findByRole("button", { name: "Detalhes de Muse Code" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Avançar" })).toBeEnabled());
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Muse Wonder");
  await user.click(screen.getByRole("button", { name: "Avançar" }));
  await user.click(screen.getByRole("button", { name: "Começar" }));
  await waitFor(() => expect(complete).toHaveBeenCalledOnce());
  expect(invoke).toHaveBeenCalledWith("set_agent_model", { flow: "standard", role: "builder", choice: { executor: "muse", account: "", model: "muse-wonder", reasoning: null } });
}, 15_000);
