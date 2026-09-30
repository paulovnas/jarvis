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
