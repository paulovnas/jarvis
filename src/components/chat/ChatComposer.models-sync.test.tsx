import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { expect, it, vi } from "vitest";
import { chatOptions } from "@/test/chat-fixtures";
import { ChatComposer } from "./ChatComposer";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

it("makes refreshed Claude login and models usable immediately after a settings-window event", async () => {
  const user = userEvent.setup();
  const send = vi.fn().mockResolvedValue(true);
  const stale = { installed: true, authenticated: false, version: "2", models: [{ id: "sonnet-old", name: "Sonnet anterior", description: "", reasoningLevels: ["high"], defaultReasoning: "high" }], error: null };
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === "get_claude_runtime") return stale;
    if (command === "get_workflow_catalog") return { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] };
    return undefined;
  });
  render(<ChatComposer modelGroups={[]} initialOptions={{ ...chatOptions, executor: "claude", account: "", model: "sonnet-new", reasoning: "high" }} onSendMessage={send} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await user.type(field, "Use o modelo atualizado");
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("claude auth login"));
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  const changed = vi.mocked(listen).mock.calls.find(([name]) => name === "claude-runtime:changed")?.[1];
  expect(changed).toBeDefined();
  const fresh = { ...stale, authenticated: true, models: [{ ...stale.models[0], id: "sonnet-new", name: "Sonnet novo" }] };
  act(() => changed?.({ event: "claude-runtime:changed", id: 1, payload: fresh }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Sonnet novo");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeEnabled();
  expect(field).toHaveTextContent("Use o modelo atualizado");
  expect(invoke).not.toHaveBeenCalledWith("refresh_claude_runtime");
  await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
  expect(send).toHaveBeenCalledWith("Use o modelo atualizado", expect.objectContaining({ executor: "claude", account: "", model: "sonnet-new", reasoning: "high" }));
});
