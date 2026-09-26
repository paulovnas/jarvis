import { useState } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ExecutorModelPicker } from "./ExecutorModelPicker";
import type { ModelSelection } from "./ModelPicker";
import type { ClaudeRuntime } from "@/core/executors";

const runtime = vi.hoisted(() => ({ data: null as ClaudeRuntime | null, loading: false, error: null as string | null, refresh: vi.fn() }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => runtime }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

beforeEach(() => {
  runtime.data = { installed: true, authenticated: true, version: "2.1", error: null, models: [{ id: "sonnet", name: "Claude Sonnet", description: "CLI", reasoningLevels: ["low", "high"], defaultReasoning: "high" }] };
  runtime.loading = false; runtime.error = null; runtime.refresh.mockClear();
});

it("switches executor explicitly and exposes the CLI model effort catalog", async () => {
  const user = userEvent.setup(); const selected = vi.fn();
  function Picker() {
    const [selection, setSelection] = useState<ModelSelection>({ model: "account/native", reasoning: null });
    return <ExecutorModelPicker selection={selection} onSelect={next => { selected(next); setSelection(next); }} modelGroups={[{ provider: "account", models: [{ value: "account/native", label: "Native", reasoningLevels: [], defaultReasoningLevel: null }] }]} />;
  }
  render(<Picker />);
  expect(screen.getByRole("button", { name: "Executor · chat" })).toHaveTextContent("Jarvis");
  screen.getByRole("button", { name: "Executor · chat" }).focus();
  await user.keyboard("{Enter}");
  await user.click(await screen.findByRole("menuitem", { name: "Claude" }));
  expect(selected).toHaveBeenLastCalledWith({ executor: "claude", model: "sonnet", reasoning: "high" });
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Claude Sonnet · Alto");
  trigger.focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Claude Code" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: /Claude Sonnet/ })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(within(await screen.findByRole("group", { name: "Raciocínio" })).getByRole("menuitem", { name: "Baixo" }));
  expect(selected).toHaveBeenLastCalledWith({ executor: "claude", model: "sonnet", reasoning: "low" });
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
