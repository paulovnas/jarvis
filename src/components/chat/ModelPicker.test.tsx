import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ModelPicker } from "./ModelPicker";

it("shows each provider icon by its kind even when aliases are arbitrary", async () => {
  const user = userEvent.setup();
  const groups = (["openai-codex", "antigravity", "custom"] as const).map((providerKind, index) => ({ provider: `Conta ${index}`, providerKind, models: [{ value: `alias-${index}/model`, label: "Modelo", reasoningLevels: [], defaultReasoningLevel: null }] }));
  render(<ModelPicker modelGroups={groups} onSelect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  const codex = await screen.findByRole("menuitem", { name: "Conta 0" });
  expect(codex.querySelector("[style]")?.getAttribute("style")).toContain("provider-openai.svg");
  expect(screen.getByRole("menuitem", { name: "Conta 1" }).querySelector("[style]")?.getAttribute("style")).toContain("provider-antigravity.svg");
  expect(screen.getByRole("menuitem", { name: "Conta 2" }).querySelector("svg.lucide-plug-zap")).toBeInTheDocument();
  expect(within(codex).queryByRole("img")).not.toBeInTheDocument();
});

it("identifies the selected provider when accounts offer the same model", () => {
  const model = { label: "GPT-5.6-Sol", reasoningLevels: ["max"], defaultReasoningLevel: "max" };
  const groups = [
    { provider: "openai-codex-pessoal", providerKind: "openai-codex" as const, models: [{ ...model, value: "openai-codex-pessoal/gpt-5.6-sol" }] },
    { provider: "openai-codex-trabalho", providerKind: "openai-codex" as const, models: [{ ...model, value: "openai-codex-trabalho/gpt-5.6-sol" }] },
  ];

  render(<ModelPicker modelGroups={groups} selection={{ model: "openai-codex-trabalho/gpt-5.6-sol", reasoning: "max" }} onSelect={vi.fn()} showProviderIdentity />);

  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("trabalho · GPT-5.6-Sol · Máximo");
  expect(trigger).not.toHaveTextContent("pessoal");
  expect(trigger.querySelector("[style]")?.getAttribute("style")).toContain("provider-openai.svg");
});

it("uses the Claude brand mark in the selected model and provider menu", async () => {
  const user = userEvent.setup();
  const models = [{ value: "sonnet", label: "Sonnet", reasoningLevels: [], defaultReasoningLevel: null }];
  render(<ModelPicker modelGroups={[{ provider: "Claude Code", executor: "claude", models }]} selection={{ executor: "claude", model: "sonnet", reasoning: null }} onSelect={vi.fn()} showProviderIdentity />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Claude Code · Sonnet");
  expect(trigger.querySelector("[style]")?.getAttribute("style")).toContain("provider-claude.svg");
  await user.click(trigger);
  const provider = await screen.findByRole("menuitem", { name: "Claude Code" });
  expect(provider.querySelector("[style]")?.getAttribute("style")).toContain("provider-claude.svg");
});

it("offers model refresh inside the menu and prevents repeated refresh while loading", async () => {
  const user = userEvent.setup();
  const refresh = vi.fn();
  const { rerender } = render(<ModelPicker modelGroups={[]} onSelect={vi.fn()} onRefresh={refresh} />);
  expect(screen.queryByRole("button", { name: /Atualizar/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  await user.click(await screen.findByRole("menuitem", { name: "Atualizar lista de modelos" }));
  expect(refresh).toHaveBeenCalledOnce();
  rerender(<ModelPicker modelGroups={[]} onSelect={vi.fn()} onRefresh={refresh} refreshing />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  const pending = await screen.findByRole("menuitem", { name: "Atualizando modelos…" });
  expect(pending).toHaveAttribute("aria-disabled", "true");
  await user.click(pending);
  expect(refresh).toHaveBeenCalledOnce();
});

it("uses an explicit label for an automatic model selection and its reset action", async () => {
  const user = userEvent.setup();
  const clear = vi.fn();
  render(<ModelPicker modelGroups={[]} onSelect={vi.fn()} onClear={clear} clearLabel="Automático" />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Automático");
  await user.click(trigger);
  await user.click(await screen.findByRole("menuitem", { name: "Automático" }));
  expect(clear).toHaveBeenCalledOnce();
});

it("removes Ultra from available efforts and allows replacing a saved legacy choice", async () => {
  const user = userEvent.setup();
  const select = vi.fn();
  render(<ModelPicker modelGroups={[{ provider: "Codex", models: [{ value: "account/model", label: "Modelo", reasoningLevels: ["low", "xhigh", "max", "ultra"], defaultReasoningLevel: "ultra" }] }]} selection={{ model: "account/model", reasoning: "ultra" }} onSelect={select} />);
  expect(select).not.toHaveBeenCalled();
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  trigger.focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Codex" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: /Modelo/ })).focus(); await user.keyboard("{ArrowRight}");
  const efforts = within(await screen.findByRole("group", { name: "Raciocínio" }));
  expect(efforts.queryByRole("menuitem", { name: "Ultra" })).not.toBeInTheDocument();
  expect(efforts.getAllByRole("menuitem").map(item => item.textContent)).toEqual(["Baixo", "Extra alto", "Máximo"]);
  await user.click(efforts.getByRole("menuitem", { name: "Máximo" }));
  expect(select).toHaveBeenCalledWith({ model: "account/model", reasoning: "max" });
});

it("keeps a model selectable with provider defaults when Ultra was its only listed mode", async () => {
  const user = userEvent.setup();
  const select = vi.fn();
  render(<ModelPicker modelGroups={[{ provider: "Codex", models: [{ value: "account/model", label: "Modelo", reasoningLevels: ["ultra"], defaultReasoningLevel: "ultra" }] }]} onSelect={select} />);
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Codex" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Modelo" }));
  expect(select).toHaveBeenCalledWith({ model: "account/model", reasoning: null });
});

const fastGroups = [{ provider: "Trabalho", providerKind: "openai-codex" as const, models: [{ value: "work/sol", label: "Sol", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high", supportsFast: true }] }, { provider: "Outro", providerKind: "custom" as const, models: [{ value: "other/sol", label: "Outro modelo", reasoningLevels: [], defaultReasoningLevel: null, supportsFast: true }] }];
async function openSpeed(user: ReturnType<typeof userEvent.setup>) {
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: /Velocidade/ })).focus(); await user.keyboard("{ArrowRight}");
}

it("offers Normal and Fast only in opted-in chats and discloses higher consumption", async () => {
  const user = userEvent.setup(), select = vi.fn();
  const selection = { model: "work/sol", reasoning: "high" };
  const view = render(<ModelPicker modelGroups={fastGroups} selection={selection} onSelect={select} allowFastMode />);
  await openSpeed(user);
  expect(await screen.findByRole("menuitemradio", { name: "Normal" })).toHaveAttribute("aria-checked", "true");
  expect(screen.getByText("Maior consumo dos limites/créditos")).toBeVisible();
  await user.click(screen.getByRole("menuitemradio", { name: "Fast" }));
  expect(select).toHaveBeenLastCalledWith({ ...selection, serviceTier: "priority" });
  view.rerender(<ModelPicker modelGroups={fastGroups} selection={{ ...selection, serviceTier: "priority" }} onSelect={select} allowFastMode />);
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Sol · Alto · Fast");
  await openSpeed(user);
  await user.click(await screen.findByRole("menuitemradio", { name: "Normal" }));
  expect(select).toHaveBeenLastCalledWith(selection);
});

it("allows a saved Fast choice to return to Normal globally without activating Fast", async () => {
  const user = userEvent.setup(), select = vi.fn();
  render(<ModelPicker modelGroups={fastGroups} selection={{ model: "work/sol", reasoning: "high", serviceTier: "priority" }} onSelect={select} />);
  expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Fast");
  await openSpeed(user);
  expect(await screen.findByRole("menuitemradio", { name: "Fast" })).toHaveAttribute("aria-disabled", "true");
  await user.click(screen.getByRole("menuitemradio", { name: "Normal" }));
  expect(select).toHaveBeenCalledExactlyOnceWith({ model: "work/sol", reasoning: "high" });
});

it("requires an advertised capability and lets unsupported saved Fast return to Normal", async () => {
  const user = userEvent.setup(), select = vi.fn();
  const groups = [{ ...fastGroups[0], models: [{ ...fastGroups[0].models[0], supportsFast: undefined }] }];
  render(<ModelPicker modelGroups={groups} selection={{ model: "work/sol", reasoning: "high", serviceTier: "priority" }} onSelect={select} allowFastMode />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Fast indisponível"); expect(trigger).toHaveAttribute("aria-invalid", "true");
  await openSpeed(user);
  expect(await screen.findByRole("menuitemradio", { name: "Fast" })).toHaveAttribute("aria-disabled", "true");
  await user.click(screen.getByRole("menuitemradio", { name: "Normal" }));
  expect(select).toHaveBeenCalledExactlyOnceWith({ model: "work/sol", reasoning: "high" });
});

it.each([undefined, false])("keeps saved Fast neutral until current OpenAI capabilities arrive (%s)", async supportsFast => {
  const user = userEvent.setup(), select = vi.fn();
  const group = { ...fastGroups[0], modelsStale: true, models: [{ ...fastGroups[0].models[0], supportsFast }] };
  const selection = { model: "work/sol", reasoning: "high", serviceTier: "priority" as const };
  const view = render(<ModelPicker modelGroups={[group]} selection={selection} onSelect={select} allowFastMode />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Sol · Alto · Fast");
  expect(trigger).not.toHaveTextContent("indisponível");
  expect(trigger).not.toHaveAttribute("aria-invalid", "true");
  await openSpeed(user);
  expect(await screen.findByRole("menuitemradio", { name: "Fast" })).toHaveAttribute("aria-disabled", "true");
  await user.click(screen.getByRole("menuitemradio", { name: "Fast" }));
  expect(select).not.toHaveBeenCalled();
  view.rerender(<ModelPicker modelGroups={[{ ...group, modelsStale: false }]} selection={selection} onSelect={select} allowFastMode />);
  expect(trigger).toHaveTextContent("Fast indisponível");
  expect(trigger).toHaveAttribute("aria-invalid", "true");
});

it.each([
  { providerKind: "custom", executor: undefined },
  { providerKind: "opencode-go", executor: undefined },
  { providerKind: "openai-codex", executor: "claude" as const },
])("reports incompatible saved Fast even with a stale $providerKind catalog and $executor executor", ({ providerKind, executor }) => {
  const group = { ...fastGroups[0], providerKind, executor, modelsStale: true };
  render(<ModelPicker modelGroups={[group]} selection={{ executor, model: "work/sol", reasoning: "high", serviceTier: "priority" }} onSelect={vi.fn()} allowFastMode />);
  const trigger = screen.getByRole("button", { name: "Selecionar modelo de IA" });
  expect(trigger).toHaveTextContent("Fast indisponível");
  expect(trigger).toHaveAttribute("aria-invalid", "true");
});

it("does not offer Fast activation from stale unsupported metadata", async () => {
  const user = userEvent.setup();
  const group = { ...fastGroups[0], modelsStale: true, models: [{ ...fastGroups[0].models[0], supportsFast: false }] };
  render(<ModelPicker modelGroups={[group]} selection={{ model: "work/sol", reasoning: "high" }} onSelect={vi.fn()} allowFastMode />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  expect(await screen.findByRole("menuitem", { name: "Trabalho" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: /Velocidade/ })).not.toBeInTheDocument();
});

it("keeps Fast on an effort change and clears it on an incompatible provider switch", async () => {
  const user = userEvent.setup(), select = vi.fn();
  render(<ModelPicker modelGroups={fastGroups} selection={{ model: "work/sol", reasoning: "high", serviceTier: "priority" }} onSelect={select} allowFastMode />);
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Trabalho" })).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem", { name: /Sol/ })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Baixo" }));
  expect(select).toHaveBeenLastCalledWith({ model: "work/sol", reasoning: "low", serviceTier: "priority" });
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem", { name: "Outro" })).focus(); await user.keyboard("{ArrowRight}");
  await user.click(await screen.findByRole("menuitem", { name: "Outro modelo" }));
  expect(select).toHaveBeenLastCalledWith({ model: "other/sol", reasoning: null });
});

it("does not expose Fast activation outside chat even when the catalog advertises it", async () => {
  const user = userEvent.setup();
  render(<ModelPicker modelGroups={fastGroups} selection={{ model: "work/sol", reasoning: "high" }} onSelect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
  expect(await screen.findByRole("menuitem", { name: "Trabalho" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: /Velocidade/ })).not.toBeInTheDocument();
});
