import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ChatComposer, type ProviderModelGroup } from "./ChatComposer";
import type { ChatDraft } from "@/core/chat";
import { chatOptions } from "@/test/chat-fixtures";
import { toast } from "sonner";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async command => command === "get_workflow_catalog" ? { revision: 0, agents: [], flows: [] } : undefined) }));

async function renderComposer(element: React.ReactElement) {
  const result = render(element);
  await screen.findByRole("textbox", { name: "Mensagem" });
  return result;
}

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
  (await screen.findAllByRole("menuitem"))[0].focus();
  await user.keyboard("{ArrowRight}");
  const item = await screen.findByRole("menuitem", { name });
  item.focus();
  await user.keyboard("{ArrowRight}");
  return screen.findByRole("group", { name: "Raciocínio" });
}

describe("ChatComposer model reasoning", () => {
  it("honors a new manual choice over an old remap and still applies later replacements", async () => {
    const source = { account: "pessoal", model: "compact", reasoning: "xhigh" };
    const bindings = [{ itemKey: "chat:c1", source, target: { account: "pessoal", model: "flexible", reasoning: "high" } }];
    const props = { modelGroups: models, onSendMessage: vi.fn(), draftKey: "c1", initialOptions: { ...chatOptions, ...source } };
    const { rerender } = await renderComposer(<ChatComposer {...props} modelBindings={bindings} />);
    const user = userEvent.setup(); const group = await openModel(user, /Compact/);
    await user.click(within(group).getByRole("menuitem", { name: "Extra alto" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_chat_model_binding", { conversationId: "c1", choice: source }));
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Compact · Extra alto");
    rerender(<ChatComposer {...props} modelBindings={[]} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Compact · Extra alto");
    rerender(<ChatComposer {...props} modelBindings={[{ itemKey: "chat:c1", source, target: { account: "pessoal", model: "plain", reasoning: null } }]} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Plain");
  });
  it("preserves the draft and reports a removed model instead of selecting another account", async () => {
    const user = userEvent.setup(); const send = vi.fn(); const notice = vi.spyOn(toast, "error");
    const initialOptions = { ...chatOptions, account: "removed", model: "old" };
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} initialOptions={initialOptions} />);
    expect(screen.getByRole("alert")).toHaveTextContent("removed/old");
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Indisponível");
    await user.type(screen.getByRole("textbox"), "Meu pedido{Enter}");
    expect(send).not.toHaveBeenCalled(); expect(screen.getByRole("textbox")).toHaveTextContent("Meu pedido");
    expect(notice).toHaveBeenCalledWith("Chat: modelo indisponível", expect.anything());
  });
  it("uses the explicit chat remap when it arrives, without switching conversations", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true);
    const initialOptions = { ...chatOptions, account: "removed", model: "old", reasoning: null };
    const { rerender } = await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} draftKey="c1" modelsReady={false} initialOptions={initialOptions} />);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.type(screen.getByRole("textbox"), "Continuar");
    const bindings = [{ itemKey: "chat:c1", source: { account: "removed", model: "old", reasoning: null }, target: { account: "pessoal", model: "compact", reasoning: "xhigh" } }];
    rerender(<ChatComposer modelGroups={models} onSendMessage={send} draftKey="c1" modelsReady initialOptions={initialOptions} modelBindings={bindings} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Compact · Extra alto");
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(send).toHaveBeenCalledWith("Continuar", expect.objectContaining({ account: "pessoal", model: "compact", reasoning: "xhigh" }));
  });
  it("selects Designer with its own model profile and sends the direct design flow", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true); const save = vi.fn();
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} agentModels={{ data: { "designer/designer": { account: "pessoal", model: "flexible", reasoning: "high" }, "planned/planner": { account: "pessoal", model: "compact", reasoning: "medium" } }, error: null, saving: false, save, refresh: vi.fn() }} />);
    await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
    await user.click(await screen.findByRole("menuitem", { name: "Designer" }));
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Flexible · Alto");
    await user.type(screen.getByRole("textbox"), "Desenhe o painel{Enter}");
    expect(send).toHaveBeenCalledWith("Desenhe o painel", { account: "pessoal", model: "flexible", reasoning: "high", mode: "build", workflow: "designer", approvalMode: "yolo" });
    const reasoning = await openModel(user, /Compact/);
    await user.click(within(reasoning).getByRole("menuitem", { name: "Extra alto" }));
    expect(save).toHaveBeenCalledWith("designer", "designer", { account: "pessoal", model: "compact", reasoning: "xhigh" });
  });
  it("mostra somente provedores e permite navegar pelos três níveis com teclado", async () => {
    const user = userEvent.setup();
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "Selecionar modelo de IA" }));
    expect((await screen.findAllByRole("menuitem")).map(item => item.textContent)).toEqual([models[0].provider]);
    screen.getByRole("menuitem", { name: models[0].provider }).focus();
    await user.keyboard("{ArrowRight}");
    const model = await screen.findByRole("menuitem", { name: /Flexible/ });
    model.focus();
    await user.keyboard("{ArrowRight}");
    await user.click(await screen.findByRole("menuitem", { name: "Alto" }));
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Flexible · Alto");
  });
  it("has no approval selector and blocks the draft during compaction", async () => {
    const user = userEvent.setup(); const send = vi.fn();
    const { rerender } = await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} />);
    expect(screen.queryByRole("button", { name: "Selecionar autorização de ferramentas" })).not.toBeInTheDocument();
    await user.type(screen.getByRole("textbox"), "Rascunho");
    rerender(<ChatComposer modelGroups={models} onSendMessage={send} compacting />);
    expect(screen.getByRole("textbox")).toHaveAttribute("contenteditable", "false");
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toBeDisabled();
    expect(screen.getByRole("group", { name: "Mensagem e opções de envio" })).toHaveAttribute("data-working", "true");
    rerender(<ChatComposer modelGroups={models} onSendMessage={send} />);
    expect(screen.getByRole("textbox")).toHaveTextContent("Rascunho");
    expect(screen.getByRole("textbox")).toHaveAttribute("contenteditable", "true");
  });
  it("allows typing and queueing during a run while locking all selectors", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true);
    const options = { ...chatOptions, account: "pessoal", model: "compact" };
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} running initialOptions={{ ...options, approvalMode: "manual" }} />);
    for (const name of ["Selecionar modelo de IA", "Selecionar fluxo"]) expect(screen.getByRole("button", { name })).toBeDisabled();
    expect(screen.getByRole("textbox")).toHaveAttribute("contenteditable", "true");
    await user.type(screen.getByRole("textbox"), "Depois verifique os testes{Enter}");
    expect(send).toHaveBeenCalledWith("Depois verifique os testes", { ...options, approvalMode: "yolo" });
    expect(screen.getByRole("textbox").textContent).toBe("");
    expect(screen.getByRole("button", { name: "Interromper execução" })).toBeEnabled();
  });

  it("does not erase text typed while an enqueue acknowledgement is pending", async () => {
    const user = userEvent.setup(); let resolve!: (value: boolean) => void;
    const send = vi.fn(() => new Promise<boolean>(done => { resolve = done; }));
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} running />);
    const field = screen.getByRole("textbox");
    await user.type(field, "Pedido{Enter}");
    expect(field).toHaveAttribute("contenteditable", "true");
    await user.type(field, " ainda digitando");
    await act(async () => resolve(true));
    expect(field).toHaveTextContent("Pedido ainda digitando");
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("restores removed queued text after the existing draft and offers resume when paused", async () => {
    const user = userEvent.setup(); const remove = vi.fn().mockResolvedValue({ content: "Verifique o build" }); const resume = vi.fn().mockResolvedValue(undefined);
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} queuedMessages={[{ id: "q1", content: "Verifique o build", options: chatOptions }]} onRemoveQueued={remove} onResumeQueue={resume} />);
    await user.type(screen.getByRole("textbox"), "Meu rascunho");
    await user.click(screen.getByRole("button", { name: "Retirar mensagem 1 e editar" }));
    expect(remove).toHaveBeenCalledWith("q1");
    expect(screen.getByRole("textbox")).toHaveTextContent("Meu rascunhoVerifique o build");
    expect(screen.getByRole("textbox")).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Continuar fila" }));
    expect(resume).toHaveBeenCalledTimes(1);
  });

  it("restores a pending cancellation to its original conversation after switching", async () => {
    const user = userEvent.setup(); const drafts = new Map<string, ChatDraft>();
    let resolve!: (value: ChatDraft) => void;
    const remove = vi.fn(() => new Promise<ChatDraft>(done => { resolve = done; }));
    const { rerender } = await renderComposer(<ChatComposer key="first" draftKey="first" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} queuedMessages={[{ id: "q1", content: "Pedido", options: chatOptions }]} onRemoveQueued={remove} />);
    await user.click(screen.getByRole("button", { name: "Retirar mensagem 1 e editar" }));
    rerender(<ChatComposer key="second" draftKey="second" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} />);
    await user.type(screen.getByRole("textbox"), "Outra conversa");
    await act(async () => resolve({ content: "Pedido" }));
    expect(screen.getByRole("textbox")).toHaveTextContent("Outra conversa");
    rerender(<ChatComposer key="first" draftKey="first" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveTextContent("Pedido"));
  });
  it("uses YOLO for legacy Manual conversations and keeps rejected drafts", async () => {
    const user = userEvent.setup();
    const send = vi.fn().mockResolvedValue(false);
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} initialOptions={{ ...chatOptions, account: "pessoal", model: "compact", reasoning: "medium", approvalMode: "manual" }} />);
    screen.getByRole("button", { name: "Selecionar fluxo" }).focus();
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("menuitem", { name: /Plan/ }));
    await user.type(screen.getByRole("textbox"), "Analise o projeto{Enter}");
    expect(send).toHaveBeenCalledWith("Analise o projeto", { account: "pessoal", model: "compact", reasoning: "medium", mode: "build", workflow: "planned", approvalMode: "yolo" });
    expect(screen.getByRole("textbox")).toHaveTextContent("Analise o projeto");
  });
  it("uses the provider default and offers only this model's levels", async () => {
    const user = userEvent.setup();
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const button = screen.getByRole("button", { name: "Selecionar modelo de IA" });
    expect(button).toHaveTextContent("Compact · Médio");

    const group = await openModel(user, /Compact/);
    expect(within(group).getAllByRole("menuitem").map(item => item.textContent)).toEqual(["Médio", "Extra alto"]);
    await user.click(within(group).getByRole("menuitem", { name: "Extra alto" }));
    expect(button).toHaveTextContent("Compact · Extra alto");
  });

  it("offers disabled and minimal reasoning only when the selected model reports them", async () => {
    const user = userEvent.setup();
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const group = await openModel(user, /Flexible/);
    expect(within(group).getAllByRole("menuitem").map(item => item.textContent)).toEqual(["Desativado", "Mínimo", "Alto"]);
    await user.click(within(group).getByRole("menuitem", { name: "Desativado" }));
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Flexible · Desativado");
  });

  it("selects a model without reported levels without inventing a reasoning menu", async () => {
    const user = userEvent.setup();
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    screen.getByRole("button", { name: "Selecionar modelo de IA" }).focus();
    await user.keyboard("{Enter}");
    (await screen.findAllByRole("menuitem"))[0].focus();
    await user.keyboard("{ArrowRight}");
    const plain = await screen.findByRole("menuitem", { name: "Plain" });
    expect(plain).not.toHaveAttribute("aria-haspopup");
    await user.click(plain);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent(/^Plain$/);
  });

  it("requires reviewing a reasoning level that is no longer available", async () => {
    const user = userEvent.setup();
    const { rerender } = await renderComposer(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const group = await openModel(user, /Compact/);
    await user.click(within(group).getByRole("menuitem", { name: "Extra alto" }));

    rerender(<ChatComposer modelGroups={[{ provider: models[0].provider, models: [{
      ...models[0].models[0], reasoningLevels: ["low", "medium"], defaultReasoningLevel: "low",
    }] }]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("Compact · Extra alto");
    expect(screen.getByRole("alert")).toHaveTextContent("indisponível");
  });

  it("keeps a removed selection visible until the user replaces it", async () => {
    const user = userEvent.setup();
    const { rerender } = await renderComposer(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} />);
    const group = await openModel(user, /Compact/);
    await user.click(within(group).getByRole("menuitem", { name: "Extra alto" }));

    rerender(<ChatComposer modelGroups={[{ provider: "OpenAI Codex · trabalho", models: [{
      ...models[0].models[0], value: "trabalho/compact", defaultReasoningLevel: "medium",
    }] }]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("compact · Indisponível");
    expect(screen.getByRole("alert")).toHaveTextContent("pessoal/compact");

    rerender(<ChatComposer modelGroups={[]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("compact · Indisponível");
  });

  it("uses the first reported level when no default exists and preserves unknown identifiers", async () => {
    const user = userEvent.setup();
    await renderComposer(<ChatComposer modelGroups={[{ provider: "OpenAI Codex", models: [{
      value: "pessoal/new", label: "New", reasoningLevels: ["future", "constructor"], defaultReasoningLevel: null,
    }] }]} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("New · future");
    const group = await openModel(user, /New/);
    await user.click(within(group).getByRole("menuitem", { name: "constructor" }));
    expect(screen.getByRole("button", { name: "Selecionar modelo de IA" })).toHaveTextContent("New · constructor");
  });
});
