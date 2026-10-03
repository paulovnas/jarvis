import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Button } from "@/components/ui/button";
import { ChatComposer } from "./ChatComposer";
import type { ChatDraft } from "@/core/chat";

const stopDictation = vi.hoisted(() => vi.fn());
vi.mock("@/hooks/use-voice", () => ({ stopDictation }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async command => command === "get_workflow_catalog" ? { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] } : undefined) }));
vi.mock("@/components/voice/VoiceControls", () => ({ VoiceSessionPanel: () => <p role="status">Estou ouvindo</p>, VoiceControls: ({ onDictation, allowCall }: { onDictation: (text: string) => void; allowCall?: boolean }) => <><Button onClick={() => onDictation("Texto ditado")}>Ditar no teste</Button>{allowCall && <Button>Ligar no teste</Button>}</> }));
const models = [{ provider: "pessoal", models: [{ value: "pessoal/model", label: "Modelo", reasoningLevels: [], defaultReasoningLevel: null }] }];

describe("composer voice adapter", () => {
  beforeEach(() => { stopDictation.mockReset().mockResolvedValue(undefined); });
  it("appends dictated text without discarding the existing draft or attachments", async () => {
    const attachment = { type: "attachment" as const, attachment: { id: "f", conversationId: "c1", name: "brief.txt", kind: "document" as const, mime: "text/plain", size: 12 } };
    const drafts = new Map<string, ChatDraft>([["c1", { content: "Minha introdução", parts: [{ type: "text", text: "Minha introdução" }, attachment] }]]);
    render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} draftKey="c1" drafts={drafts} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Ditar no teste" })));
    expect(drafts.get("c1")?.content).toContain("Minha introdução"); expect(drafts.get("c1")?.content).toContain("Texto ditado");
    expect(drafts.get("c1")?.parts).toContainEqual(attachment);
  });
  it("offers dictation without calls and places feedback above the toolbar", async () => {
    const send = vi.fn().mockResolvedValue(true);
    const drafts = new Map<string, ChatDraft>([["c1", { content: "Rascunho ainda não enviado" }]]);
    render(<ChatComposer modelGroups={models} onSendMessage={send} draftKey="c1" drafts={drafts} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    expect(screen.queryByRole("button", { name: "Ligar no teste" })).not.toBeInTheDocument();
    const feedback = screen.getByRole("status");
    const toolbar = screen.getByRole("button", { name: "Mais ações" }).closest(".composer-controls");
    expect(toolbar).not.toContainElement(feedback);
    expect(feedback.parentElement?.nextElementSibling).toBe(toolbar);
    expect(send).not.toHaveBeenCalled();
    expect(drafts.get("c1")?.content).toBe("Rascunho ainda não enviado");
  });
  it.each([false, true])("stops dictation before accepting a message, including queue submissions (running=%s)", async running => {
    let stopped: () => void = () => {};
    stopDictation.mockImplementation(() => new Promise<void>(resolve => { stopped = resolve; }));
    const send = vi.fn().mockResolvedValue(true);
    const drafts = new Map<string, ChatDraft>([["c1", { content: "Texto ditado" }]]);
    render(<ChatComposer modelGroups={models} onSendMessage={send} draftKey="c1" drafts={drafts} running={running} />);
    const editor = await screen.findByRole("textbox", { name: "Mensagem" });
    await act(async () => { fireEvent.keyDown(editor, { key: "Enter" }); });
    expect(stopDictation).toHaveBeenCalledExactlyOnceWith("chat:c1");
    expect(send).not.toHaveBeenCalled();
    await act(async () => { stopped(); });
    expect(send).toHaveBeenCalledWith("Texto ditado", expect.objectContaining({ approvalMode: "yolo" }));
    expect(drafts.has("c1")).toBe(false);
  });
  it("preserves the submitted draft when sending fails after capture has stopped", async () => {
    const send = vi.fn().mockResolvedValue(false);
    const drafts = new Map<string, ChatDraft>([["c1", { content: "Texto ditado" }]]);
    render(<ChatComposer modelGroups={models} onSendMessage={send} draftKey="c1" drafts={drafts} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Enviar mensagem" })); });
    expect(stopDictation).toHaveBeenCalledExactlyOnceWith("chat:c1");
    expect(drafts.get("c1")?.content).toBe("Texto ditado");
  });
  it("keeps the draft and does not send if the microphone cannot be stopped", async () => {
    stopDictation.mockRejectedValue("Microfone indisponível");
    const send = vi.fn(), drafts = new Map<string, ChatDraft>([["c1", { content: "Texto ditado" }]]);
    render(<ChatComposer modelGroups={models} onSendMessage={send} draftKey="c1" drafts={drafts} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Enviar mensagem" })); });
    expect(send).not.toHaveBeenCalled();
    expect(drafts.get("c1")?.content).toBe("Texto ditado");
  });
});
