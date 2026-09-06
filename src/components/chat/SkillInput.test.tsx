import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatComposer } from "./ChatComposer";
import { chatOptions } from "@/test/chat-fixtures";
import type { ChatDraft, MessagePart } from "@/core/chat";
import type { Skill } from "@/core/skills";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const models = [{ provider: "pessoal", models: [{ value: "pessoal/test", label: "Test", reasoningLevels: [], defaultReasoningLevel: null }] }];
const react: Skill = { id: "react", name: "react-expert", description: "Interfaces React", origin: "jarvis", path: "/skills/react", enabled: true, automatic: true, source: null, marketplaceId: null, updateAvailable: false, updateError: null };
const manual: Skill = { ...react, id: "manual", name: "review", description: "Revisão manual", origin: "project", automatic: false };
const skillParts: MessagePart[] = [{ type: "skill", id: "react", name: "react-expert" }, { type: "text", text: " Revise a tela" }];
const snapshot = { includeAgents: false, directory: "/skills", skills: [react, manual, { ...react, id: "off", name: "disabled-skill", enabled: false }], warnings: [] };

async function renderComposer(element: React.ReactElement) {
  const result = render(element);
  await screen.findByRole("textbox", { name: "Mensagem" });
  return result;
}

describe("Explicit skill input", () => {
  beforeEach(() => { vi.mocked(invoke).mockReset().mockResolvedValue(snapshot); });
  it("filtra skills ativas por /, seleciona com teclado e envia a badge como referência", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true);
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} />);
    const field = screen.getByRole("textbox", { name: "Mensagem" });
    expect(field).toHaveAttribute("spellcheck", "true");
    expect(field).toHaveAttribute("autocorrect", "on");
    expect(field).toHaveAttribute("autocapitalize", "sentences");
    await user.type(field, "/");
    expect(await screen.findByRole("option", { name: /react-expert/ })).toBeVisible();
    expect(screen.getByRole("option", { name: /review/ })).toBeVisible();
    expect(screen.queryByRole("option", { name: /disabled-skill/ })).not.toBeInTheDocument();
    await user.keyboard("rev");
    expect(screen.queryByRole("option", { name: /react-expert/ })).not.toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(within(field).getByRole("button", { name: "Remover skill review" })).toBeVisible();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(send).not.toHaveBeenCalled();
    await user.keyboard("Confira os testes{Enter}");
    expect(send).toHaveBeenCalledWith("/review Confira os testes", expect.any(Object), [{ type: "skill", id: "manual", name: "review" }, { type: "text", text: " Confira os testes" }]);
    await waitFor(() => expect(field.textContent).toBe(""));
  });
  it("seleciona por clique, remove a badge e preserva o texto ao redor", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(false);
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} />);
    const field = screen.getByRole("textbox");
    await user.type(field, "Use /rea");
    await user.click(await screen.findByRole("option", { name: /react-expert/ }));
    await user.keyboard("aqui");
    await user.click(within(field).getByRole("button", { name: "Remover skill react-expert" }));
    expect(field).toHaveTextContent("Use aqui");
    await user.keyboard("{Enter}");
    expect(send).toHaveBeenCalledWith("Use  aqui", expect.any(Object));
  });
  it("Escape fecha o seletor, setas escolhem e Tab insere sem enviar", async () => {
    const user = userEvent.setup(); const send = vi.fn(); await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} />);
    await user.type(screen.getByRole("textbox"), "/");
    await screen.findByRole("option", { name: /react-expert/ });
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    await user.keyboard("{Control>}a{/Control}{Backspace}");
    await waitFor(() => expect(screen.getByRole("textbox").textContent).toBe(""));
    await user.keyboard("/");
    await screen.findByRole("option", { name: /react-expert/ });
    await user.keyboard("{ArrowDown}{Tab}");
    expect(screen.getByRole("button", { name: "Remover skill review" })).toBeVisible();
    expect(send).not.toHaveBeenCalled();
  });
  it("mostra skeleton durante descoberta e impede envio acidental ao confirmar lista vazia", async () => {
    let resolve!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementation(() => new Promise(done => { resolve = done; }));
    const user = userEvent.setup(); const send = vi.fn(); await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} />);
    await user.type(screen.getByRole("textbox"), "/");
    expect(screen.getByRole("status", { name: "Carregando skills" })).toBeVisible();
    await user.keyboard("{Enter}"); expect(send).not.toHaveBeenCalled();
    await act(async () => resolve({ ...snapshot, skills: [] }));
    expect(screen.getByText("Nenhuma skill encontrada.")).toBeVisible();
    await user.keyboard("{Enter}"); expect(send).not.toHaveBeenCalled();
  });
  it("mantém badges na fila e restaura o rascunho completo na conversa original", async () => {
    const user = userEvent.setup(); const drafts = new Map<string, ChatDraft>();
    let resolve!: (value: ChatDraft) => void;
    const remove = vi.fn(() => new Promise<ChatDraft>(done => { resolve = done; }));
    const { rerender } = await renderComposer(<ChatComposer key="one" draftKey="one" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} queuedMessages={[{ id: "q", content: "/react-expert Revise a tela", parts: skillParts, options: chatOptions }]} onRemoveQueued={remove} />);
    expect(within(screen.getByRole("region", { name: "Mensagens agendadas" })).getByTitle("Skill: react-expert")).toBeVisible();
    await user.type(screen.getByRole("textbox"), "Antes");
    await user.click(screen.getByRole("button", { name: "Retirar mensagem 1 e editar" }));
    rerender(<ChatComposer key="two" draftKey="two" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} />);
    await user.type(screen.getByRole("textbox"), "Outra conversa");
    await act(async () => resolve({ content: "/react-expert Revise a tela", parts: skillParts }));
    expect(screen.getByRole("textbox")).toHaveTextContent("Outra conversa");
    rerender(<ChatComposer key="one" draftKey="one" drafts={drafts} modelGroups={models} onSendMessage={vi.fn()} />);
    expect(screen.getByRole("textbox")).toHaveTextContent("Antes");
    expect(await screen.findByRole("button", { name: "Remover skill react-expert" })).toBeVisible();
    expect(screen.getByRole("textbox")).toHaveTextContent("Revise a tela");
  });
  it("preserva acentos e quebras de linha ao colar e não envia durante composição", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(false);
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} />);
    const field = screen.getByRole("textbox");
    await user.click(field);
    await user.paste("ação opção português\nsegunda linha");
    fireEvent.keyDown(field, { key: "Enter", isComposing: true });
    expect(send).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(send).toHaveBeenCalledWith("ação opção português\nsegunda linha", expect.any(Object));
  });
  it("oculta o placeholder na mesma transação de colar e restaura ao apagar ou enviar", async () => {
    const user = userEvent.setup(); const send = vi.fn().mockResolvedValue(true);
    await renderComposer(<ChatComposer modelGroups={models} onSendMessage={send} />);
    const field = screen.getByRole("textbox", { name: "Mensagem" });
    expect(field).toHaveAttribute("data-empty", "true");
    await user.click(field);
    await user.paste("Texto colado\n");
    expect(field).toHaveTextContent("Texto colado");
    expect(field).toHaveAttribute("data-empty", "false");
    await user.keyboard("{Control>}a{/Control}{Backspace}");
    expect(field).toHaveAttribute("data-empty", "true");
    await user.paste("Outra mensagem");
    expect(field).toHaveAttribute("data-empty", "false");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(field).toHaveAttribute("data-empty", "true"));
  });
});
