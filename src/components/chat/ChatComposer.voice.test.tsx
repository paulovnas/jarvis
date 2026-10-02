import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button } from "@/components/ui/button";
import { ChatComposer } from "./ChatComposer";
import type { ChatDraft } from "@/core/chat";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async command => command === "get_workflow_catalog" ? { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] } : undefined) }));
vi.mock("@/components/voice/VoiceControls", () => ({ VoiceSessionPanel: () => null, VoiceControls: ({ onDictation, onMessage }: { onDictation: (text: string) => void; onMessage: (text: string) => Promise<boolean> }) => <><Button onClick={() => onDictation("Texto ditado")}>Ditar no teste</Button><Button onClick={() => { void onMessage("Pedido falado"); }}>Falar no teste</Button></> }));
const models = [{ provider: "pessoal", models: [{ value: "pessoal/model", label: "Modelo", reasoningLevels: [], defaultReasoningLevel: null }] }];

describe("composer voice adapter", () => {
  it("appends dictated text without discarding the existing draft or attachments", async () => {
    const attachment = { type: "attachment" as const, attachment: { id: "f", conversationId: "c1", name: "brief.txt", kind: "document" as const, mime: "text/plain", size: 12 } };
    const drafts = new Map<string, ChatDraft>([["c1", { content: "Minha introdução", parts: [{ type: "text", text: "Minha introdução" }, attachment] }]]);
    render(<ChatComposer modelGroups={models} onSendMessage={vi.fn()} draftKey="c1" drafts={drafts} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Ditar no teste" })));
    expect(drafts.get("c1")?.content).toContain("Minha introdução"); expect(drafts.get("c1")?.content).toContain("Texto ditado");
    expect(drafts.get("c1")?.parts).toContainEqual(attachment);
  });
  it("sends a voice message through the selected chat pipeline while retaining a separate typed draft", async () => {
    const send = vi.fn().mockResolvedValue(true);
    const drafts = new Map<string, ChatDraft>([["c1", { content: "Rascunho ainda não enviado" }]]);
    render(<ChatComposer modelGroups={models} onSendMessage={send} draftKey="c1" drafts={drafts} />);
    await screen.findByRole("textbox", { name: "Mensagem" });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Falar no teste" })));
    await waitFor(() => expect(send).toHaveBeenCalledWith("Pedido falado", expect.objectContaining({ account: "pessoal", model: "model", approvalMode: "yolo" })));
    expect(drafts.get("c1")?.content).toBe("Rascunho ainda não enviado");
  });
});
