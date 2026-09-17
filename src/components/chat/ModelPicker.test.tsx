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
