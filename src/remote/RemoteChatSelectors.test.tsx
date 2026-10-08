import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { ProviderModelGroup } from "@/components/chat/ModelPicker";
import type { ModelChoice } from "@/core/provider-references";
import { customAgent, customCatalog, customFlow } from "@/test/workflow-fixtures";
import { RemoteChatSelectors, type RemoteChatSelectorsProps } from "./RemoteChatSelectors";

const models: ProviderModelGroup[] = [
  { provider: "ene", providerKind: "openai-codex", models: [
    { value: "ene/gpt-sol", label: "GPT Sol", reasoningLevels: ["low", "high", "max", "ultra"], defaultReasoningLevel: "high" },
    { value: "ene/gpt-luna", label: "GPT Luna", reasoningLevels: ["low", "high"], defaultReasoningLevel: "low" },
  ] },
  { provider: "Claude Code", executor: "claude", models: [{ value: "sonnet", label: "Sonnet", reasoningLevels: [], defaultReasoningLevel: null }] },
];
const choice: ModelChoice = { account: "ene", model: "gpt-sol", reasoning: "high", fallback: { executor: "claude", account: "", model: "sonnet", reasoning: null } };
const props = (overrides: Partial<RemoteChatSelectorsProps> = {}): RemoteChatSelectorsProps => ({ catalog: customCatalog, modelGroups: models, flow: "standard", choice, onFlowChange: vi.fn(), onModelChange: vi.fn(), ...overrides });

it("selects custom flows and individual agents without offering flow-only agents", async () => {
  const user = userEvent.setup(), onFlowChange = vi.fn();
  const current = props({ onFlowChange, catalog: { ...customCatalog, agents: [...customCatalog.agents, { ...customAgent, id: "d".repeat(32), name: "Agente interno", usage: "flow_only" }] } });
  render(<RemoteChatSelectors {...current} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo ou agente" }));
  expect(await screen.findByRole("dialog", { name: "Fluxos e agentes" })).toBeVisible();
  expect(screen.queryByRole("option", { name: "Agente interno" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("option", { name: customAgent.name }));
  expect(onFlowChange).toHaveBeenCalledExactlyOnceWith(`agent:${customAgent.id}`);
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo ou agente" }));
  await user.click(await screen.findByRole("option", { name: customFlow.name }));
  expect(onFlowChange).toHaveBeenLastCalledWith(`custom:${customFlow.id}`);
});

it("searches provider and model labels, stages reasoning, and preserves the secondary", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ onModelChange })} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  expect(await screen.findByRole("dialog", { name: "Modelo desta conversa" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Ultra" })).not.toBeInTheDocument();
  const search = screen.getByRole("combobox", { name: "Buscar modelo" });
  await user.type(search, "GPT Luna");
  expect(screen.getAllByRole("option")).toHaveLength(1);
  await user.click(screen.getByRole("option", { name: "ene · GPT Luna" }));
  await user.click(screen.getByRole("button", { name: "Alto" }));
  expect(onModelChange).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith({ executor: "jarvis", account: "ene", model: "gpt-luna", reasoning: "high", fallback: choice.fallback });
});

it("swaps primary and secondary when the opposite execution target is selected", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ onModelChange })} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("option", { name: "Claude Code · Sonnet" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith({ ...choice.fallback, fallback: { account: "ene", model: "gpt-sol", reasoning: "high" } });
});

it("allows clearing the optional secondary without changing the primary", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ onModelChange })} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("button", { name: "Secundário" }));
  await user.click(screen.getByRole("option", { name: "Nenhum" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith({ ...choice, fallback: null });
});

it("highlights an unavailable saved model and requires a valid replacement", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ onModelChange, choice: { account: "retired", model: "missing", reasoning: null }, modelProblem: "O provedor foi removido. Selecione um modelo." })} />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveAttribute("aria-invalid", "true");
  expect(screen.queryByText("O provedor foi removido. Selecione um modelo.")).not.toBeInTheDocument();
  await user.click(trigger);
  expect(await screen.findByRole("alert")).toHaveTextContent("O provedor foi removido. Selecione um modelo.");
  expect(await screen.findByRole("button", { name: "Aplicar modelo" })).toBeDisabled();
  await user.click(screen.getByRole("option", { name: "ene · GPT Sol" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith({ executor: "jarvis", account: "ene", model: "gpt-sol", reasoning: "high" });
});

it("disables controls during a pending mutation and retains the sheet on rejection", async () => {
  const user = userEvent.setup();
  let resolve: (value: boolean) => void = () => {};
  const onModelChange = vi.fn(() => new Promise<boolean>(done => { resolve = done; }));
  render(<RemoteChatSelectors {...props({ onModelChange })} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("button", { name: "Aplicar modelo" }));
  expect(screen.getByRole("button", { name: "Salvando…" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Fechar seleção" })).toBeDisabled();
  expect(screen.getByRole("option", { name: "ene · GPT Luna" })).toHaveAttribute("aria-disabled", "true");
  await act(async () => { resolve(false); });
  expect(screen.getByRole("dialog", { name: "Modelo desta conversa" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Aplicar modelo" })).toBeEnabled();
  expect(onModelChange).toHaveBeenCalledTimes(1);
});

it("does not publish staged model changes when the selector is dismissed", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ onModelChange })} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("option", { name: "ene · GPT Luna" }));
  await user.click(screen.getByRole("button", { name: "Fechar seleção" }));
  expect(onModelChange).not.toHaveBeenCalled();
});

it("disables both compact triggers while remote choices are loading or saving", () => {
  render(<RemoteChatSelectors {...props({ disabled: true })} />);
  expect(screen.getByRole("button", { name: "Selecionar fluxo ou agente" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toBeDisabled();
});

it("preserves the active flow while allowing the next message model to change", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn(), onFlowChange = vi.fn();
  render(<RemoteChatSelectors {...props({ flowDisabled: true, onModelChange, onFlowChange })} />);
  expect(screen.getByRole("button", { name: "Selecionar fluxo ou agente" })).toBeDisabled();
  const model = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(model).toBeEnabled();
  await user.click(model);
  await user.click(await screen.findByRole("option", { name: "ene · GPT Luna" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith({ executor: "jarvis", account: "ene", model: "gpt-luna", reasoning: "low", fallback: choice.fallback });
  expect(onFlowChange).not.toHaveBeenCalled();
});

const fastModels = () => models.map(group => ({ ...group, models: group.models.map(model => ({ ...model, supportsFast: group.providerKind === "openai-codex" && model.value === "ene/gpt-sol" })) }));
it("stages Fast and reasoning only in this chat, discards on close, and applies after review", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ modelGroups: fastModels(), onModelChange })} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("button", { name: "Fast" }));
  expect(screen.getByText("Maior consumo dos limites/créditos")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Fechar seleção" }));
  expect(onModelChange).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  expect(await screen.findByRole("button", { name: "Normal" })).toHaveAttribute("aria-pressed", "true");
  await user.click(screen.getByRole("button", { name: "Fast" }));
  await user.click(screen.getByRole("button", { name: "Baixo" }));
  expect(onModelChange).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith({ ...choice, reasoning: "low", serviceTier: "priority" });
});

it("shows restored Fast and clears it when choosing an unsupported model", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ modelGroups: fastModels(), choice: { ...choice, serviceTier: "priority" }, onModelChange })} />);
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("GPT Sol · Fast");
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("option", { name: "ene · GPT Luna" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith({ executor: "jarvis", account: "ene", model: "gpt-luna", reasoning: "low", fallback: choice.fallback });
});

it("blocks unsupported saved Fast until Normal is explicitly chosen", async () => {
  const user = userEvent.setup(), onModelChange = vi.fn();
  render(<RemoteChatSelectors {...props({ choice: { ...choice, serviceTier: "priority" }, onModelChange })} />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Fast indisponível"); expect(trigger).toHaveAttribute("aria-invalid", "true");
  await user.click(trigger);
  expect(await screen.findByRole("button", { name: "Fast" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Aplicar modelo" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Normal" }));
  await user.click(screen.getByRole("button", { name: "Aplicar modelo" }));
  expect(onModelChange).toHaveBeenCalledExactlyOnceWith(choice);
});
