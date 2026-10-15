import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { ChatComposer } from "./ChatComposer";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const text = "Pode implementar os próximos passos que você sugeriu.";
const events = new Map<string, (payload: unknown) => void>();
const modelGroups = [{ provider: "chat-provider", models: [{ value: "chat-provider/model", label: "Modelo", reasoningLevels: [], defaultReasoningLevel: null }] }];
const snapshot = () => ({ ...emptyChat(), turns: [savedTurn()] });
beforeEach(() => {
  events.clear();
  vi.mocked(listen).mockImplementation(async (event, handler) => { events.set(String(event), payload => handler({ event: String(event), id: 1, payload })); return () => { events.delete(String(event)); }; });
  vi.mocked(invoke).mockReset().mockImplementation(async command => command === "get_chat_prediction" ? { turnId: "turn1", text } : command === "get_workflow_catalog" ? { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] } : null);
});

it("offers ghost text without storing a draft and accepts Tab without sending", async () => {
  const send = vi.fn();
  const user = userEvent.setup();
  render(<ChatComposer draftKey="c1" historySnapshot={snapshot()} modelGroups={modelGroups} onSendMessage={send} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(field).toHaveAttribute("data-placeholder", text));
  expect(field.textContent).toBe("");
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  field.focus(); await user.keyboard("{Shift>}{Tab}{/Shift}");
  expect(field.textContent).toBe("");
  expect(invoke).not.toHaveBeenCalledWith("dismiss_chat_prediction", expect.anything());
  field.focus(); await user.keyboard("{Tab}");
  await waitFor(() => expect(field).toHaveTextContent(text));
  expect(send).not.toHaveBeenCalled();
  expect(invoke).toHaveBeenCalledWith("dismiss_chat_prediction", { conversationId: "c1", turnId: "turn1" });
});

it("discards a prediction on typing and does not restore it after deleting the draft", async () => {
  const user = userEvent.setup();
  render(<ChatComposer draftKey="c1" historySnapshot={snapshot()} modelGroups={modelGroups} onSendMessage={vi.fn()} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(field).toHaveAttribute("data-placeholder", text));
  field.focus(); await user.paste("Meu próximo pedido");
  await user.keyboard("{Control>}a{/Control}{Backspace}");
  await act(async () => events.get("chat:prediction_changed")?.({ conversationId: "c1" }));
  expect(field.textContent).toBe("");
  expect(field).not.toHaveAttribute("data-placeholder", text);
});

it("ignores a late prediction when typing began before it arrived", async () => {
  let complete!: (value: unknown) => void;
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args, options) => command === "get_chat_prediction" ? new Promise(resolve => { complete = resolve; }) : original(command, args, options));
  const user = userEvent.setup();
  render(<ChatComposer draftKey="c1" historySnapshot={snapshot()} modelGroups={modelGroups} onSendMessage={vi.fn()} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(complete).toBeDefined());
  field.focus(); await user.paste("Outro plano");
  await user.keyboard("{Control>}a{/Control}{Backspace}");
  await act(async () => complete({ turnId: "turn1", text }));
  expect(field).not.toHaveAttribute("data-placeholder", text);
});

it("allows a new prediction only for the next completed delivery", async () => {
  const user = userEvent.setup();
  const props = { draftKey: "c1", modelGroups, onSendMessage: vi.fn() };
  const view = render(<ChatComposer {...props} historySnapshot={snapshot()} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await waitFor(() => expect(field).toHaveAttribute("data-placeholder", text));
  field.focus(); await user.paste("Novo pedido");
  await user.keyboard("{Control>}a{/Control}{Backspace}");
  vi.mocked(invoke).mockResolvedValue({ turnId: "turn2", text: "Agora pode adicionar os testes." });
  view.rerender(<ChatComposer {...props} historySnapshot={{ ...snapshot(), revision: 2, turns: [...snapshot().turns, { ...savedTurn(), id: "turn2" }] }} />);
  await waitFor(() => expect(field).toHaveAttribute("data-placeholder", "Agora pode adicionar os testes."));
});
