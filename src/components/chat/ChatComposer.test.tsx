import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";
import { chatOptions } from "@/test/chat-fixtures";

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
  it("shows only the policy names and blocks the draft during compaction", async () => {
    const user = userEvent.setup(); const send = vi.fn();
    const { rerender } = render(<ChatComposer modelGroups={models} onSendMessage={send} />);
    screen.getByRole("button", { name: "Selecionar autorização de ferramentas" }).focus();
    await user.keyboard("{Enter}");
    expect((await screen.findAllByRole("menuitem")).map(item => item.textContent?.trim())).toEqual(["Manual", "YOLO"]);
    await user.keyboard("{Escape}");
    await user.type(screen.getByRole("textbox"), "Rascunho");
    rerender(<ChatComposer modelGroups={models} onSendMessage={send} compacting />);
    expect(screen.getByRole("textbox")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toBeDisabled();
    expect(screen.getByRole("group", { name: "Mensagem e opções de envio" })).toHaveAttribute("data-working", "true");
    rerender(<ChatComposer modelGroups={models} onSendMessage={send} />);
    expect(screen.getByRole("textbox")).toHaveValue("Rascunho");
    expect(screen.getByRole("textbox")).toBeEnabled();
  });
  it("allows typing and queueing during a run while locking all selectors", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true);
    render(<ChatComposer modelGroups={models} onSendMessage={send} running initialOptions={chatOptions} />);
    for (const name of ["Selecionar modelo de IA", "Selecionar modo de execução", "Selecionar autorização de ferramentas"]) expect(screen.getByRole("button", { name })).toBeDisabled();
    expect(screen.getByRole("textbox")).toBeEnabled();
    await user.type(screen.getByRole("textbox"), "Depois verifique os testes{Enter}");
    expect(send).toHaveBeenCalledWith("Depois verifique os testes", chatOptions);
    expect(screen.getByRole("textbox")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Interromper execução" })).toBeEnabled();
  });

  it("does not erase text typed while an enqueue acknowledgement is pending", async () => {
    const user = userEvent.setup(); let resolve!: (value: boolean) => void;
    const send = vi.fn(() => new Promise<boolean>(done => { resolve = done; }));
    render(<ChatComposer modelGroups={models} onSendMessage={send} running />);
    const field = screen.getByRole("textbox");
    await user.type(field, "Pedido{Enter}");
    expect(field).toBeEnabled();
    await user.type(field, " ainda digitando");
    await act(async () => resolve(true));
    expect(field).toHaveValue("Pedido ainda digitando");
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("restores removed queued text after the existing draft and offers resume when paused", async () => {
    const user = userEvent.setup(); const remove = vi.fn().mockResolvedValue("Verifique o build"); const resume = vi.fn().mockResolvedValue(undefined);
    render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} queuedMessages={[{ id: "q1", content: "Verifique o build", options: chatOptions }]} onRemoveQueued={remove} onResumeQueue={resume} />);
    await user.type(screen.getByRole("textbox"), "Meu rascunho");
    await user.click(screen.getByRole("button", { name: "Retirar mensagem 1 e editar" }));
    expect(remove).toHaveBeenCalledWith("q1");
    expect(screen.getByRole("textbox")).toHaveValue("Meu rascunho\n\nVerifique o build");
    expect(screen.getByRole("textbox")).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Continuar fila" }));
    expect(resume).toHaveBeenCalledTimes(1);
  });

  it("restores a pending cancellation to its original conversation after switching", async () => {
    const user = userEvent.setup(); const drafts = new Map<string, string>();
    let resolve!: (value: string) => void;
    const remove = vi.fn(() => new Promise<string>(done => { resolve = done; }));
    const { rerender } = render(<ChatComposer key="first" draftKey="first" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} queuedMessages={[{ id: "q1", content: "Pedido", options: chatOptions }]} onRemoveQueued={remove} />);
    await user.click(screen.getByRole("button", { name: "Retirar mensagem 1 e editar" }));
    rerender(<ChatComposer key="second" draftKey="second" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} />);
    await user.type(screen.getByRole("textbox"), "Outra conversa");
    await act(async () => resolve("Pedido"));
    expect(screen.getByRole("textbox")).toHaveValue("Outra conversa");
    rerender(<ChatComposer key="first" draftKey="first" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveValue("Pedido"));
  });
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
