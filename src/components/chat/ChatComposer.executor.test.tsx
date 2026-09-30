import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ChatComposer } from "./ChatComposer";
import { chatOptions } from "@/test/chat-fixtures";
import type { ClaudeRuntime } from "@/core/executors";
import type { AgyRuntime } from "@/core/agy";
import type { ProviderModelGroup } from "./ModelPicker";

const runtime = vi.hoisted(() => ({ data: null as ClaudeRuntime | null, loading: false, error: null as string | null, refresh: vi.fn() }));
const agy = vi.hoisted(() => ({ data: null as AgyRuntime | null, loading: false, error: null as string | null, refresh: vi.fn() }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => runtime }));
vi.mock("@/hooks/use-agy-runtime", () => ({ useAgyRuntime: () => agy }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async command => command === "get_workflow_catalog" ? { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] } : undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
beforeEach(() => {
  runtime.data = { installed: true, authenticated: true, version: "2", error: null, models: [{ id: "sonnet", name: "Claude Sonnet", description: "", reasoningLevels: ["high"], defaultReasoning: "high" }] };
  runtime.loading = false; runtime.error = null;
  agy.data = null; agy.loading = false; agy.error = null;
});

it("sends AGY turns without native providers while retaining the primary executor identity", async () => {
  agy.data = { installed: true, authenticated: true, version: "1.2.13", error: null, preferences: { enabled: true, disabledModels: [] }, models: [{ id: "gemini", name: "Gemini", description: "", reasoningLevels: ["high"], defaultReasoning: "high" }] };
  const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true);
  render(<ChatComposer modelGroups={[]} modelsReady={false} initialOptions={{ ...chatOptions, executor: "agy", account: "", model: "gemini", reasoning: "high" }} onSendMessage={send} />);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Implemente{Enter}");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(send).toHaveBeenCalledWith("Implemente", expect.objectContaining({ executor: "agy", account: "", model: "gemini", reasoning: "high" }));
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Antigravity CLI");
});

it.each(["disabled", "login", "missing"])("retains the AGY draft when CLI requires %s configuration", async state => {
  agy.data = { installed: state !== "missing", authenticated: state !== "login", version: "1.2.13", error: null, preferences: { enabled: state !== "disabled", disabledModels: [] }, models: [{ id: "gemini", name: "Gemini", description: "", reasoningLevels: [], defaultReasoning: null }] };
  const user = userEvent.setup(); const send = vi.fn();
  render(<ChatComposer modelGroups={[]} initialOptions={{ ...chatOptions, executor: "agy", account: "", model: "gemini", reasoning: null }} onSendMessage={send} />);
  await user.type(await screen.findByRole("textbox", { name: "Mensagem" }), "Continue{Enter}");
  expect(screen.getByRole("alert")).toHaveTextContent(state === "disabled" ? "Ative o Antigravity CLI" : state === "missing" ? "Instale o Antigravity CLI" : "Execute agy no terminal");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  expect(screen.getByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Continue");
  expect(send).not.toHaveBeenCalled();
});

it("swaps an AGY secondary into the primary profile through the shared selector", async () => {
  agy.data = { installed: true, authenticated: true, version: "1.2.13", error: null, preferences: { enabled: true, disabledModels: [] }, models: [{ id: "gemini", name: "Gemini", description: "", reasoningLevels: [], defaultReasoning: null }] };
  const user = userEvent.setup(); const save = vi.fn();
  const primary = { executor: "claude" as const, account: "", model: "sonnet", reasoning: "high" };
  const fallback = { executor: "agy" as const, account: "", model: "gemini", reasoning: null };
  render(<ChatComposer modelGroups={[]} onSendMessage={vi.fn()} agentModels={{ data: { "standard/builder": { ...primary, fallback } }, saving: false, error: null, save, refresh: vi.fn() }} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Antigravity CLI" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Gemini" }));
  expect(save).toHaveBeenCalledWith("standard", "builder", { ...fallback, fallback: primary });
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
