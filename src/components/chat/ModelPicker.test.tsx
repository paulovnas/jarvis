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
