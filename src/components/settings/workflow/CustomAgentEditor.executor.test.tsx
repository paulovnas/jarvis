import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { CustomAgentEditor } from "./CustomAgentEditor";
import { customAgent } from "@/test/workflow-fixtures";

vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => ({ data: { installed: false, authenticated: false, version: null, models: [], error: null }, loading: false, error: null, refresh: vi.fn() }) }));

it("saves a custom Claude agent before CLI setup without requiring a Jarvis provider", async () => {
  const user = userEvent.setup(); const save = vi.fn().mockResolvedValue(true); const close = vi.fn();
  render(<CustomAgentEditor initial={customAgent} accounts={[]} saving={false} creating={false} onSave={save} onClose={close} />);
  await user.click(screen.getByRole("button", { name: "Modelo do agente customizado" }));
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Padrão do Claude Code" }));
  expect(screen.getByText("Modelo fixo: Claude Code / default")).toBeVisible();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Salvar agente" }));
  await waitFor(() => expect(save).toHaveBeenCalledExactlyOnceWith({ ...customAgent, model: { executor: "claude", account: "", model: "default", reasoning: null } }));
  expect(close).toHaveBeenCalledOnce();
});
