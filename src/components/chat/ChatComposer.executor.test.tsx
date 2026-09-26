import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ChatComposer } from "./ChatComposer";
import { chatOptions } from "@/test/chat-fixtures";
import type { ClaudeRuntime } from "@/core/executors";

const runtime = vi.hoisted(() => ({ data: null as ClaudeRuntime | null, loading: false, error: null as string | null, refresh: vi.fn() }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => runtime }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async command => command === "get_workflow_catalog" ? { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] } : undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
beforeEach(() => {
  runtime.data = { installed: true, authenticated: true, version: "2", error: null, models: [{ id: "sonnet", name: "Claude Sonnet", description: "", reasoningLevels: ["high"], defaultReasoning: "high" }] };
  runtime.loading = false; runtime.error = null;
});

it("sends with the Claude executor and empty account without requiring native providers", async () => {
  const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true);
  render(<ChatComposer modelGroups={[]} modelsReady={false} initialOptions={{ ...chatOptions, executor: "claude", account: "", model: "sonnet", reasoning: "high" }} onSendMessage={send} />);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Implemente a melhoria{Enter}");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(send).toHaveBeenCalledWith("Implemente a melhoria", expect.objectContaining({ executor: "claude", account: "", model: "sonnet", reasoning: "high" }));
});

it("retains the draft and asks for CLI login instead of a missing provider", async () => {
  runtime.data!.authenticated = false;
  const user = userEvent.setup(); const send = vi.fn();
  render(<ChatComposer modelGroups={[]} initialOptions={{ ...chatOptions, executor: "claude", account: "", model: "sonnet", reasoning: "high" }} onSendMessage={send} />);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Continue{Enter}");
  expect(screen.getByRole("alert")).toHaveTextContent("claude auth login");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Continue");
  expect(send).not.toHaveBeenCalled();
});

it("persists Claude choices in native profiles instead of fabricating an account", async () => {
  const user = userEvent.setup(); const save = vi.fn();
  render(<ChatComposer modelGroups={[]} onSendMessage={vi.fn()} agentModels={{ data: {}, saving: false, error: null, save, refresh: vi.fn() }} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  screen.getByRole("button", { name: "Executor · chat" }).focus(); await user.keyboard("{Enter}");
  await user.click(await screen.findByRole("menuitem", { name: "Claude" }));
  expect(save).toHaveBeenCalledWith("standard", "builder", { executor: "claude", account: "", model: "sonnet", reasoning: "high" });
});
