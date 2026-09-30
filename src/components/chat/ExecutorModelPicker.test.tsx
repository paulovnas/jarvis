import { useState } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ExecutorModelPicker } from "./ExecutorModelPicker";
import type { ModelSelection } from "./ModelPicker";
import type { ClaudeRuntime } from "@/core/executors";
import type { AgyRuntime } from "@/core/agy";

const runtime = vi.hoisted(() => ({ data: null as ClaudeRuntime | null, loading: false, error: null as string | null, refresh: vi.fn() }));
const agy = vi.hoisted(() => ({ data: null as AgyRuntime | null, loading: false, error: null as string | null, refresh: vi.fn() }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => runtime }));
vi.mock("@/hooks/use-agy-runtime", () => ({ useAgyRuntime: () => agy }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

beforeEach(() => {
  runtime.data = { installed: true, authenticated: true, version: "2.1", error: null, models: [{ id: "sonnet", name: "Claude Sonnet", description: "CLI", reasoningLevels: ["low", "high"], defaultReasoning: "high" }] };
  runtime.loading = false; runtime.error = null; runtime.refresh.mockClear();
  agy.data = null; agy.loading = false; agy.error = null; agy.refresh.mockClear();
});

it("refreshes the selected AGY catalog through the CLI and keeps the direct Antigravity provider separate", async () => {
  agy.data = { installed: true, authenticated: true, version: "1.2.13", error: null, preferences: { enabled: true, disabledModels: [] }, models: [{ id: "gemini", name: "Gemini CLI", description: "", reasoningLevels: [], defaultReasoning: null }] };
  const user = userEvent.setup(); const nativeRefresh = vi.fn(); const select = vi.fn();
  render(<ExecutorModelPicker selection={{ executor: "agy", model: "gemini", reasoning: null }} onSelect={select} onRefresh={nativeRefresh} modelGroups={[{ provider: "antigravity-direct", providerKind: "antigravity", models: [{ value: "antigravity-direct/gemini", label: "Gemini direto", reasoningLevels: [], defaultReasoningLevel: null }] }]} showProviderIdentity />);
  const picker = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(picker).toHaveTextContent("Antigravity CLI · Gemini CLI");
  expect(picker.querySelector("[style]")?.getAttribute("style")).toContain("provider-antigravity.svg");
  picker.focus(); await user.keyboard("{Enter}");
  expect(await screen.findByRole("menuitem", { name: "antigravity-direct" })).toBeVisible();
  expect(screen.getByRole("menuitem", { name: "Antigravity CLI" })).toBeVisible();
  await user.click(screen.getByRole("menuitem", { name: "Atualizar lista de modelos" }));
  expect(agy.refresh).toHaveBeenCalledOnce();
  expect(nativeRefresh).not.toHaveBeenCalled();
  picker.focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "antigravity-direct" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Gemini direto" }));
  expect(select).toHaveBeenCalledWith({ executor: "jarvis", model: "antigravity-direct/gemini", reasoning: null });
});

it("selects Claude and native providers through the same menu and preserves their execution routes", async () => {
  const user = userEvent.setup(); const selected = vi.fn();
  function Picker() {
    const [selection, setSelection] = useState<ModelSelection>({ model: "account/native", reasoning: null });
    return <ExecutorModelPicker selection={selection} onSelect={next => { selected(next); setSelection(next); }} modelGroups={[{ provider: "account", models: [{ value: "account/native", label: "Native", reasoningLevels: [], defaultReasoningLevel: null }] }]} />;
  }
  render(<Picker />);
  expect(screen.queryByRole("button", { name: /Executor/ })).not.toBeInTheDocument();
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  trigger.focus(); await user.keyboard("{Enter}");
  expect(await screen.findByRole("menuitem", { name: "account" })).toBeVisible();
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: /Claude Sonnet/ })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(within(await screen.findByRole("group", { name: "Raciocínio" })).getByRole("menuitem", { name: "Baixo" }));
  expect(selected).toHaveBeenLastCalledWith({ executor: "claude", model: "sonnet", reasoning: "low" });
  expect(trigger).toHaveTextContent("Claude Sonnet · Baixo");
  trigger.focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "account" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Native" }));
  expect(selected).toHaveBeenLastCalledWith({ executor: "jarvis", model: "account/native", reasoning: null });
});

it("keeps saved AGY variants configurable through the grouped model and its advertised efforts", async () => {
  agy.data = { installed: true, authenticated: true, version: "1.2.13", error: null, preferences: { enabled: true, disabledModels: [] }, models: [{ id: "gemini-3.8-flash", name: "Gemini 3.8 Flash", description: "", reasoningLevels: ["low", "high"], defaultReasoning: "high" }] };
  const user = userEvent.setup(); const selected = vi.fn();
  render(<ExecutorModelPicker selection={{ executor: "agy", model: "gemini-3.8-flash-high", reasoning: null }} onSelect={selected} modelGroups={[]} />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Gemini 3.8 Flash · Alto");
  expect(trigger).not.toHaveTextContent("Indisponível");
  trigger.focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Antigravity CLI" })).focus(); await user.keyboard("{ArrowRight}");
  const models = await screen.findAllByRole("menuitem", { name: /Gemini 3.8 Flash/ });
  expect(models).toHaveLength(1);
  models[0].focus(); await user.keyboard("{ArrowRight}");
  const efforts = within(await screen.findByRole("group", { name: "Raciocínio" }));
  expect(efforts.getAllByRole("menuitem").map(item => item.textContent)).toEqual(["Baixo", "Alto"]);
  await user.click(efforts.getByRole("menuitem", { name: "Baixo" }));
  expect(selected).toHaveBeenCalledWith({ executor: "agy", model: "gemini-3.8-flash", reasoning: "low" });
});

it("offers plain AGY thinking models without inventing an effort submenu", async () => {
  agy.data = { installed: true, authenticated: true, version: "1.2.13", error: null, preferences: { enabled: true, disabledModels: [] }, models: [{ id: "gemini-3.8-flash-thinking", name: "Gemini 3.8 Flash Thinking", description: "", reasoningLevels: [], defaultReasoning: null }] };
  const user = userEvent.setup(); const selected = vi.fn();
  render(<ExecutorModelPicker selection={{ executor: "agy", model: "gemini-3.8-flash-thinking", reasoning: null }} onSelect={selected} modelGroups={[]} />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Gemini 3.8 Flash Thinking");
  trigger.focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Antigravity CLI" })).focus(); await user.keyboard("{ArrowRight}");
  const model = await screen.findByRole("menuitem", { name: "Gemini 3.8 Flash Thinking" });
  expect(model).not.toHaveAttribute("aria-haspopup", "menu");
  expect(screen.queryByRole("group", { name: "Raciocínio" })).not.toBeInTheDocument();
  await user.click(model);
  expect(selected).toHaveBeenCalledWith({ executor: "agy", model: "gemini-3.8-flash-thinking", reasoning: null });
});

it("filters hidden Claude models and removes the disabled local provider", async () => {
  runtime.data!.preferences = { enabled: true, disabledModels: ["sonnet"] };
  const user = userEvent.setup();
  const { rerender } = render(<ExecutorModelPicker modelGroups={[]} onSelect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  expect(screen.queryByRole("menuitem", { name: "Claude Sonnet" })).not.toBeInTheDocument();
  expect(await screen.findByText(/Configure os modelos em Configurações/)).toBeVisible();
  await user.keyboard("{Escape}{Escape}");
  runtime.data!.preferences.enabled = false;
  rerender(<ExecutorModelPicker modelGroups={[]} onSelect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  expect(screen.queryByRole("menuitem", { name: "Claude Code" })).not.toBeInTheDocument();
});

it("offers native login guidance and explicit discovery refresh when disconnected", async () => {
  runtime.data = { installed: false, authenticated: false, version: null, models: [], error: null };
  const user = userEvent.setup();
  render(<ExecutorModelPicker selection={{ executor: "claude", model: "default", reasoning: null }} onSelect={vi.fn()} modelGroups={[]} />);
  await user.click(screen.getByRole("button", { name: "Status e configuração do Claude Code" }));
  const dialog = screen.getByRole("dialog");
  expect(dialog).toHaveTextContent("Claude Code não encontrado");
  expect(dialog).toHaveTextContent("claude auth login");
  expect(within(dialog).getByRole("button", { name: "Instalar Claude Code" })).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "Atualizar status e modelos" }));
  expect(runtime.refresh).toHaveBeenCalledOnce();
});

it("keeps a hidden default model unavailable even before the local CLI is installed", async () => {
  runtime.data = { installed: false, authenticated: false, version: null, models: [], error: null, preferences: { enabled: true, disabledModels: ["default"] } };
  const user = userEvent.setup();
  render(<ExecutorModelPicker modelGroups={[]} onSelect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  expect(await screen.findByText(/Configure os modelos em Configurações/)).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: "Padrão do Claude Code" })).not.toBeInTheDocument();
});
