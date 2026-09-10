import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatComposer } from "./ChatComposer";
import type { ChatDraft } from "@/core/chat";
import type { Attachment } from "@/core/attachments";
import { chatOptions } from "@/test/chat-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const file: Attachment = { id: "image1", conversationId: "conversation", name: "screen.png", kind: "image", mime: "image/png", size: 100 };
const modelGroups = [{ provider: "pessoal", models: [{ value: "pessoal/gpt-5.6-luna", label: "Luna", reasoningLevels: [], defaultReasoningLevel: null }] }];
beforeEach(() => { vi.mocked(invoke).mockReset().mockImplementation(async command => command === "import_chat_attachments" ? [file] : "data:image/png;base64,dGVzdA=="); });
describe("Composer attachments", () => {
  it("pastes images above the text and keeps the reference while typing and sending", async () => {
    const send = vi.fn().mockResolvedValue(true); const user = userEvent.setup();
    render(<ChatComposer modelGroups={modelGroups} onSendMessage={send} draftKey="conversation" />);
    const field = await screen.findByRole("textbox", { name: "Mensagem" });
    fireEvent.paste(field, { clipboardData: { files: [new File(["image"], "screen.png", { type: "image/png" })], getData: () => "" } });
    expect(await screen.findByRole("button", { name: "Remover anexo screen.png" })).toBeVisible();
    await user.type(field, "Leia a imagem");
    expect(screen.getByRole("button", { name: "Remover anexo screen.png" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(send).toHaveBeenCalledWith("Leia a imagem", expect.anything(), expect.arrayContaining([{ type: "attachment", attachment: file }]));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Remover anexo screen.png" })).not.toBeInTheDocument());
  });
  it("accepts documents from the picker and supports attachment-only drafts and removal", async () => {
    const doc: Attachment = { ...file, name: "notes.txt", kind: "document", mime: "text/plain" };
    vi.mocked(invoke).mockResolvedValue([doc]);
    const user = userEvent.setup(); const drafts = new Map<string, ChatDraft>(); const send = vi.fn().mockResolvedValue(false);
    render(<ChatComposer modelGroups={modelGroups} onSendMessage={send} draftKey="conversation" drafts={drafts} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    await user.upload(screen.getByLabelText("Selecionar anexos"), new File(["hello"], "notes.txt", { type: "text/plain" }));
    await screen.findByRole("button", { name: "Remover anexo notes.txt" });
    expect(drafts.get("conversation")?.parts).toContainEqual({ type: "attachment", attachment: doc });
    await user.click(screen.getByRole("button", { name: "Enviar mensagem" }));
    expect(send).toHaveBeenCalledWith("Analise os anexos.", expect.anything(), expect.arrayContaining([{ type: "attachment", attachment: doc }]));
    await user.click(screen.getByRole("button", { name: "Remover anexo notes.txt" }));
    expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  });
  it("blocks sending during import and restores queued attachments into the draft", async () => {
    let finish!: (value: Attachment[]) => void;
    vi.mocked(invoke).mockImplementation(command => command === "import_chat_attachments" ? new Promise(resolve => { finish = resolve; }) : Promise.resolve("data:image/png;base64,dGVzdA=="));
    const user = userEvent.setup(); const send = vi.fn();
    const removed = { content: "Confira", parts: [{ type: "text" as const, text: "Confira" }, { type: "attachment" as const, attachment: file }] };
    render(<ChatComposer modelGroups={modelGroups} onSendMessage={send} draftKey="conversation" running queuedMessages={[{ id: "q", ...removed, options: chatOptions }]} onRemoveQueued={vi.fn().mockResolvedValue(removed)} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    await user.click(screen.getByRole("button", { name: "Editar mensagem 1" }));
    expect(screen.getByRole("button", { name: "Remover anexo screen.png" })).toBeVisible();
    await user.upload(screen.getByLabelText("Selecionar anexos"), new File(["another"], "notes.txt", { type: "text/plain" }));
    await waitFor(() => expect(finish).toBeDefined());
    expect(screen.getByRole("button", { name: "Agendar mensagem" })).toBeDisabled();
    await act(async () => finish([{ ...file, id: "doc", name: "notes.txt", kind: "document" }]));
    expect(screen.getByRole("button", { name: "Agendar mensagem" })).toBeEnabled();
  });
});
