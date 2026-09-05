import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";

const models: ProviderModelGroup[] = [{
  provider: "OpenAI Codex · pessoal",
  models: [
    { value: "pessoal/compact", label: "Compact", reasoningLevels: ["medium", "xhigh"], defaultReasoningLevel: "medium" },
    { value: "pessoal/flexible", label: "Flexible", reasoningLevels: ["none", "minimal", "high"], defaultReasoningLevel: "minimal" },
    { value: "pessoal/plain", label: "Plain", reasoningLevels: [], defaultReasoningLevel: null },
  ],
}];

async function openModel(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus();
  await user.keyboard("{Enter}");
  const item = await screen.findByRole("menuitem", { name });
  item.focus();
  await user.keyboard("{ArrowRight}");
  return screen.findByRole("group", { name: "Raciocínio" });
}

describe("ChatComposer model reasoning", () => {
  it("sends the selected Manual policy and Plan mode and keeps rejected drafts", async () => {
    const user = userEvent.setup();
    const send = vi.fn().mockResolvedValue(false);
    render(<ChatComposer modelGroups={models} onSendMessage={send} />);
    screen.getByRole("button", { name: "Selecionar autorização de ferramentas" }).focus();
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("menuitem", { name: /Manual/ }));
    screen.getByRole("button", { name: "Selecionar modo de execução" }).focus();
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("menuitem", { name: /Plan/ }));
    await user.type(screen.getByRole("textbox"), "Analise o projeto{Enter}");
    expect(send).toHaveBeenCalledWith("Analise o projeto", { account: "pessoal", model: "compact", reasoning: "medium", mode: "plan", approvalMode: "manual" });
    expect(screen.getByRole("textbox")).toHaveValue("Analise o projeto");
  });
  it("uses the provider default and offers only this model's levels", async () => {
    const user = userEvent.setup();
    render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const button = screen.getByRole("button", { name: "Selecionar modelo de IA" });
    expect(button).toHaveTextContent("Compact · Médio");

    const group = await openModel(user, /Compact/);
    expect(within(group).getAllByRole("menuitem").map(item => item.textContent)).toEqual(["Médio", "Extra alto"]);
    await user.click(within(group).getByRole("menuitem", { name: "Extra alto" }));
    expect(button).toHaveTextContent("Compact · Extra alto");
  });

  it("offers disabled and minimal reasoning only when the selected model reports them", async () => {
    const user = userEvent.setup();
    render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const group = await openModel(user, /Flexible/);
    expect(within(group).getAllByRole("menuitem").map(item => item.textContent)).toEqual(["Desativado", "Mínimo", "Alto"]);
    await user.click(within(group).getByRole("menuitem", { name: "Desativado" }));
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Flexible · Desativado");
  });

  it("selects a model without reported levels without inventing a reasoning menu", async () => {
    const user = userEvent.setup();
    render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus();
    await user.keyboard("{Enter}");
    const plain = await screen.findByRole("menuitem", { name: "Plain" });
    expect(plain).not.toHaveAttribute("aria-haspopup");
    await user.click(plain);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent(/^Plain$/);
  });

  it("falls back to the current model's default when a selected level disappears", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const group = await openModel(user, /Compact/);
    await user.click(within(group).getByRole("menuitem", { name: "Extra alto" }));

    rerender(<ChatComposer modelGroups={[{ provider: models[0].provider, models: [{
      ...models[0].models[0], reasoningLevels: ["low", "medium"], defaultReasoningLevel: "low",
    }] }]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Compact · Baixo");
  });

  it("uses the remaining account's default after the selected account is removed", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const group = await openModel(user, /Compact/);
    await user.click(within(group).getByRole("menuitem", { name: "Extra alto" }));

    rerender(<ChatComposer modelGroups={[{ provider: "OpenAI Codex · trabalho", models: [{
      ...models[0].models[0], value: "trabalho/compact", defaultReasoningLevel: "medium",
    }] }]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Compact · Médio");

    rerender(<ChatComposer modelGroups={[]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Nenhum modelo conectado");
  });

  it("uses the first reported level when no default exists and preserves unknown identifiers", async () => {
    const user = userEvent.setup();
    render(<ChatComposer modelGroups={[{ provider: "OpenAI Codex", models: [{
      value: "pessoal/new", label: "New", reasoningLevels: ["future", "constructor"], defaultReasoningLevel: null,
    }] }]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("New · future");
    const group = await openModel(user, /New/);
    await user.click(within(group).getByRole("menuitem", { name: "constructor" }));
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("New · constructor");
  });
});
