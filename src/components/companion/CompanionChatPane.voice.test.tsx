import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { companionChatSchema } from "@/core/companion";
import { CompanionChatPane } from "./CompanionChatPane";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string) => {
  if (command === "get_companion_chat" || command === "send_companion_message") return companionChatSchema.parse({ conversationId: "general", global: true, projectId: null, projectName: null, proposal: null, chat: { conversationId: "general", revision: 1, turns: [], activeTurnId: null, pendingApproval: null } });
  return [];
}) }));
vi.mock("@/components/chat/ModelPicker", () => ({ ModelPicker: () => <div aria-label="Modelo do Jarvito">Modelo atual</div> }));

describe("Jarvito text conversation", () => {
  beforeEach(() => vi.mocked(invoke).mockClear());
  it("keeps the conversation and model picker visible without microphone or phone controls", async () => {
    render(<CompanionChatPane />);
    await screen.findByRole("textbox", { name: "Mensagem para Jarvito" });
    expect(screen.getByRole("log", { name: "Mensagens do Jarvito" })).toBeVisible();
    expect(screen.getByLabelText("Modelo do Jarvito")).toBeVisible();
    expect(screen.getByRole("button", { name: "Limpar conversa" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Ditar|Ligar|Encerrar ligação|Pausar microfone|Mostrar conversa|Voltar à ligação/ })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Ligação com Jarvito")).not.toBeInTheDocument();
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "get_voice_settings" || command === "start_voice_session")).toBe(false);
  });
  it("sends a typed message using the regular companion conversation", async () => {
    render(<CompanionChatPane />);
    const input = await screen.findByRole("textbox", { name: "Mensagem para Jarvito" });
    fireEvent.change(input, { target: { value: "Pode conferir a previsão?" } });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Enviar mensagem" })); });
    expect(invoke).toHaveBeenCalledWith("send_companion_message", { conversationId: null, content: "Pode conferir a previsão?" });
    expect(input).toHaveValue("");
  });
});
