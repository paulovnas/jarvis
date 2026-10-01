import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ChatComposer } from "./ChatComposer";
import { chatOptions } from "@/test/chat-fixtures";
import type { ClaudeRuntime } from "@/core/executors";
import type { ProviderModelGroup } from "./ModelPicker";

const runtime = vi.hoisted(() => ({ data: null as ClaudeRuntime | null, loading: false, error: null as string | null, refresh: vi.fn() }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => runtime }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async command => command === "get_workflow_catalog" ? { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] } : undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
beforeEach(() => {
  runtime.data = { installed: true, authenticated: true, version: "2", error: null, models: [{ id: "sonnet", name: "Claude Sonnet", description: "", reasoningLevels: ["high"], defaultReasoning: "high" }] };
  runtime.loading = false; runtime.error = null;
});

it("keeps a retired executor unavailable without losing the draft or choosing another provider", async () => {
  const user = userEvent.setup(); const send = vi.fn();
  const modelGroups: ProviderModelGroup[] = [{ provider: "direct", providerKind: "antigravity", models: [{ value: "direct/gemini", label: "Gemini direto", reasoningLevels: [], defaultReasoningLevel: null }] }];
  render(<ChatComposer modelGroups={modelGroups} initialOptions={{ ...chatOptions, executor: "unavailable", account: "", model: "gemini", reasoning: null }} onSendMessage={send} />);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Continue{Enter}");
  expect(screen.getByRole("alert")).toHaveTextContent("Este executor foi removido");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Continue");
  expect(send).not.toHaveBeenCalled();
});

it("requires replacing a retired secondary configuration without changing the working primary", async () => {
  const user = userEvent.setup(); const send = vi.fn();
  const primary = { executor: "claude" as const, account: "", model: "sonnet", reasoning: "high", fallback: { executor: "unavailable" as const, account: "", model: "gemini", reasoning: null } };
  render(<ChatComposer modelGroups={[]} onSendMessage={send} agentModels={{ data: { "standard/builder": primary }, saving: false, error: null, save: vi.fn(), refresh: vi.fn() }} />);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Continue{Enter}");
  expect(screen.getByRole("alert")).toHaveTextContent("Este executor foi removido");
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Claude Sonnet");
  expect(send).not.toHaveBeenCalled();
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
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: /^Claude Sonnet/ })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
  expect(save).toHaveBeenCalledWith("standard", "builder", { executor: "claude", account: "", model: "sonnet", reasoning: "high" });
});

it("preserves a selected Claude profile and draft when its provider is disabled", async () => {
  runtime.data!.preferences = { enabled: false, disabledModels: [] };
  const user = userEvent.setup(); const send = vi.fn();
  render(<ChatComposer modelGroups={[]} initialOptions={{ ...chatOptions, executor: "claude", account: "", model: "sonnet", reasoning: "high" }} onSendMessage={send} />);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Continue{Enter}");
  expect(screen.getByRole("alert")).toHaveTextContent("Ative o Claude Code em Configurações → Provedores.");
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Continue");
  expect(send).not.toHaveBeenCalled();
});

it("preserves the configured secondary when the composer changes the native primary model", async () => {
  const user = userEvent.setup(); const save = vi.fn();
  const fallback = { account: "backup", model: "other", reasoning: null };
  render(<ChatComposer modelGroups={[]} onSendMessage={vi.fn()} agentModels={{ data: { "standard/builder": { executor: "claude", account: "", model: "sonnet", reasoning: "high", fallback } }, saving: false, error: null, save, refresh: vi.fn() }} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: /^Claude Sonnet/ })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
  expect(save).toHaveBeenCalledWith("standard", "builder", { executor: "claude", account: "", model: "sonnet", reasoning: "high", fallback });
});

it("swaps the configured secondary into the primary selection in the chat", async () => {
  const user = userEvent.setup(); const save = vi.fn();
  const primary = { executor: "jarvis" as const, account: "work", model: "gpt", reasoning: null };
  const secondary = { executor: "claude" as const, account: "", model: "sonnet", reasoning: "high" };
  const modelGroups: ProviderModelGroup[] = [{ provider: "work", models: [{ value: "work/gpt", label: "GPT", reasoningLevels: [], defaultReasoningLevel: null }] }];
  const controller = { data: { "standard/builder": { ...primary, fallback: secondary } }, saving: false, error: null, save, refresh: vi.fn() };
  const { rerender } = render(<ChatComposer modelGroups={modelGroups} onSendMessage={vi.fn()} agentModels={controller} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: /^Claude Sonnet/ })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
  const swapped = { ...secondary, fallback: primary };
  expect(save).toHaveBeenCalledWith("standard", "builder", swapped);
  rerender(<ChatComposer modelGroups={modelGroups} onSendMessage={vi.fn()} agentModels={{ ...controller, data: { "standard/builder": swapped } }} />);
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Claude Sonnet");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});
