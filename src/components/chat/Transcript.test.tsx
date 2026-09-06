import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { populatedLibrary } from "@/test/library-fixtures";
import { useChat } from "@/hooks/use-chat";
import { ChatArea } from "./ChatArea";
import type { HistoryPage } from "@/core/chat";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
function page(start: number, id = "c1"): HistoryPage {
  return { conversationId: id, history: { start, total: 100 }, compactions: [], navigation: [0, 10, 20, 30, 40, 50, 60, 70, 80, 99].map(index => ({ id: `t${index}`, index, createdAt: 1, user: `Pedido ${index}`, assistant: `Resumo ${index}` })), turns: Array.from({ length: 20 }, (_, i) => ({ ...savedTurn(), id: `t${start + i}`, user: `Pedido ${start + i}`, steps: [{ ...savedTurn().steps[0], text: `Resposta ${start + i}`, summary: "", tools: [] }] })) };
}
function Harness({ id = "c1" }: { id?: string }) {
  const library = populatedLibrary(); library.conversations[0].id = id; library.selection.conversationId = id;
  return <ChatArea library={library} chat={useChat(id)} modelGroups={[{ provider: "Codex", models: [{ value: "openai-codex-pessoal/model", label: "Modelo", reasoningLevels: ["medium"], defaultReasoningLevel: "medium" }] }]} />;
}
describe("lazy transcript navigation", () => {
  beforeEach(() => { call.mockReset(); vi.mocked(listen).mockResolvedValue(() => {}); });
  it("oculta a navegação com menos de dez trechos", async () => {
    call.mockResolvedValue({ ...emptyChat(), ...page(80), navigation: page(80).navigation.slice(0, 9) });
    render(<Harness />);
    await screen.findByText("Resposta 99");
    expect(screen.queryByRole("navigation", { name: "Navegar pela conversa" })).not.toBeInTheDocument();
  });
  it("ignores elastic overscroll but loads older messages on a real upward scroll", async () => {
    call.mockImplementation(async command => command === "get_chat_history" ? page(60) : { ...emptyChat(), ...page(80) });
    const { container } = render(<Harness />);
    await screen.findByText("Resposta 99");
    const viewport = container.querySelector<HTMLElement>('.transcript-scroll [data-slot="scroll-area-viewport"]')!;
    Object.defineProperties(viewport, { scrollHeight: { configurable: true, value: 1600 }, clientHeight: { configurable: true, value: 600 } });
    viewport.scrollTop = 400; fireEvent.scroll(viewport);
    viewport.scrollTop = -40; fireEvent.scroll(viewport);
    viewport.scrollTop = 1100; fireEvent.scroll(viewport);
    expect(call.mock.calls.filter(([command]) => command === "get_chat_history")).toHaveLength(0);
    viewport.scrollTop = 100; fireEvent.scroll(viewport);
    await screen.findByText("Resposta 60");
    expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", before: 80 });
  });
  it("loads only the initial page, jumps by excerpt, and keeps the composer draft", async () => {
    call.mockImplementation(async (command, args) => {
      if (command === "get_chat") return { ...emptyChat(), ...page(80) };
      if (command === "get_chat_history") return page(args && "around" in args ? 0 : 80);
      return [];
    });
    const user = userEvent.setup(); render(<Harness />);
    expect(await screen.findByText("Pedido 99")).toBeInTheDocument();
    expect(screen.queryByText("Resposta 0")).not.toBeInTheDocument();
    await user.type(await screen.findByRole("textbox"), "Meu rascunho");
    const first = screen.getByRole("button", { name: "Ir para interação 1: Pedido 0" });
    act(() => first.focus());
    expect(await screen.findByText("Resumo 0")).toBeInTheDocument();
    await user.click(first);
    expect(await screen.findByText("Resposta 0")).toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("get_chat_history", { conversationId: "c1", around: 0 });
    expect(screen.queryByText("Resposta 99")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveTextContent("Meu rascunho");
    await user.click(screen.getByRole("button", { name: "Voltar ao presente" }));
    expect(await screen.findByText("Resposta 99")).toBeInTheDocument();
  }, 15000);
  it("discards a history response after switching conversation", async () => {
    let resolve: (value: unknown) => void = () => {};
    call.mockImplementation(async (command, args) => command === "get_chat_history" ? new Promise(done => { resolve = done; }) : { ...emptyChat(args && "conversationId" in args ? String(args.conversationId) : "c1"), ...page(80, args && "conversationId" in args ? String(args.conversationId) : "c1") });
    const user = userEvent.setup(); const { rerender } = render(<Harness />);
    await user.click(await screen.findByRole("button", { name: "Mensagens anteriores" }));
    expect(screen.getByRole("status", { name: "Carregando trecho" })).toBeInTheDocument();
    rerender(<Harness id="c2" />);
    await waitFor(() => expect(call).toHaveBeenCalledWith("get_chat", { conversationId: "c2" }));
    await act(async () => resolve(page(60)));
    expect(screen.queryByText("Resposta 60")).not.toBeInTheDocument();
    expect(await screen.findByText("Resposta 99")).toBeInTheDocument();
  });
});
