import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { ChatDraft } from "@/core/chat";
import { ChatComposer } from "./ChatComposer";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), info: vi.fn(), dismiss: vi.fn() } }));
const modelGroups = [{ provider: "chat-provider", models: [{ value: "chat-provider/chat-model", label: "Chat model", reasoningLevels: [], defaultReasoningLevel: null }] }];
const catalog = { revision: 0, agents: [], flows: [], builtinAgents: [], builtinFlows: [] };
beforeEach(() => {
  vi.mocked(invoke).mockReset().mockImplementation(async command => command === "get_workflow_catalog" ? catalog : undefined);
  vi.mocked(toast.error).mockReset();
  vi.mocked(toast.info).mockReset();
});

it.each([84, 256])("keeps the blurred draft at height %i, prevents concurrent actions, and replaces only the draft", async height => {
  let finish!: (value: string) => void;
  vi.mocked(invoke).mockImplementation(async command => command === "refine_chat_prompt" ? new Promise<string>(resolve => { finish = resolve; }) : catalog);
  const send = vi.fn();
  const drafts = new Map<string, ChatDraft>([["c1", { content: "Explique sem alterar código" }]]);
  const user = userEvent.setup();
  render(<ChatComposer draftKey="c1" drafts={drafts} modelGroups={modelGroups} onSendMessage={send} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  Object.defineProperty(field, "clientHeight", { configurable: true, value: height });
  Object.defineProperty(field, "scrollHeight", { configurable: true, value: 1000 });
  field.style.lineHeight = "24px";
  field.style.paddingTop = "16px";
  field.style.paddingBottom = "8px";
  const refine = screen.getByRole("button", { name: "Refinar prompt" });
  await user.click(refine);
  const loading = await screen.findByRole("status", { name: "Refinando prompt" });
  expect(loading.querySelectorAll('[data-slot="skeleton"]')).toHaveLength(0);
  expect(loading).toHaveTextContent("Refinando o texto");
  expect(field).toHaveTextContent("Explique sem alterar código");
  expect(field.parentElement).not.toHaveClass("invisible");
  // jsdom has no layout; guard the clipping and action-space contracts here.
  expect(loading.closest(".chat-composer")).toHaveClass("overflow-hidden");
  expect(loading.parentElement).toHaveClass("overflow-hidden");
  expect(screen.queryByRole("textbox", { name: "Mensagem" })).not.toBeInTheDocument();
  expect(field).toHaveAttribute("aria-disabled", "true");
  expect(screen.getByRole("button", { name: "Cancelar refinamento" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Refinar prompt" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Enviar mensagem" })).toBeDisabled();
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "refine_chat_prompt")).toEqual([["refine_chat_prompt", { prompt: "Explique sem alterar código", conversationId: "c1", requestId: expect.any(String) }]]);
  await act(async () => finish("Objetivo: explicar o projeto.\nRestrição: não alterar código.\nResposta: explicação concisa."));
  await waitFor(() => expect(field).toBeVisible());
  expect(field).toHaveTextContent("Restrição: não alterar código.");
  expect(drafts.get("c1")?.content).toContain("Objetivo: explicar o projeto.");
  expect(screen.queryByRole("status", { name: "Refinando prompt" })).not.toBeInTheDocument();
  expect(send).not.toHaveBeenCalled();
  expect(refine).toBeEnabled();
});

it.each([null, { code: "prompt_refinement_error", message: "Escolha um modelo em Configurações → Geral → Refinar prompt." }])("preserves the original draft after unusable output or missing configuration: %j", async response => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command !== "refine_chat_prompt") return catalog;
    if (response) throw response;
    return "";
  });
  const send = vi.fn();
  const drafts = new Map<string, ChatDraft>([["c1", { content: "Meu pedido original" }]]);
  render(<ChatComposer draftKey="c1" drafts={drafts} modelGroups={modelGroups} onSendMessage={send} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await userEvent.setup().click(screen.getByRole("button", { name: "Refinar prompt" }));
  await waitFor(() => expect(toast.error).toHaveBeenCalled());
  expect(field).toBeVisible();
  expect(field).toHaveTextContent("Meu pedido original");
  expect(drafts.get("c1")?.content).toBe("Meu pedido original");
  expect(screen.getByRole("button", { name: "Refinar prompt" })).toBeEnabled();
  expect(send).not.toHaveBeenCalled();
});

it("finishes in the original draft while another conversation is visible", async () => {
  let finish!: (value: string) => void;
  vi.mocked(invoke).mockImplementation(async command => command === "refine_chat_prompt" ? new Promise<string>(resolve => { finish = resolve; }) : catalog);
  const drafts = new Map<string, ChatDraft>([["c1", { content: "Rascunho A" }], ["c2", { content: "Rascunho B" }]]);
  const props = { drafts, modelGroups, onSendMessage: vi.fn() };
  const view = render(<ChatComposer {...props} key="c1" draftKey="c1" />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  await userEvent.setup().click(screen.getByRole("button", { name: "Refinar prompt" }));
  await screen.findByRole("status", { name: "Refinando prompt" });
  view.rerender(<ChatComposer {...props} key="c2" draftKey="c2" />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await act(async () => finish("Resposta tardia A"));
  expect(field).toHaveTextContent("Rascunho B");
  expect(drafts.get("c1")?.content).toBe("Resposta tardia A");
  expect(drafts.get("c2")?.content).toBe("Rascunho B");
  view.rerender(<ChatComposer {...props} key="c1" draftKey="c1" />);
  expect(await screen.findByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Resposta tardia A");
});

it("restores a pending refinement after navigating away and back without starting another request", async () => {
  let finish!: (value: string) => void;
  vi.mocked(invoke).mockImplementation(async command => command === "refine_chat_prompt" ? new Promise<string>(resolve => { finish = resolve; }) : catalog);
  const drafts = new Map<string, ChatDraft>([["c1", { content: "Original A" }], ["c2", { content: "Original B" }]]);
  const props = { drafts, modelGroups, onSendMessage: vi.fn() };
  const view = render(<ChatComposer {...props} key="c1" draftKey="c1" />);
  await userEvent.setup().click(await screen.findByRole("button", { name: "Refinar prompt" }));
  await screen.findByRole("status", { name: "Refinando prompt" });
  view.rerender(<ChatComposer {...props} key="c2" draftKey="c2" />);
  expect(await screen.findByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Original B");
  view.rerender(<ChatComposer {...props} key="c1" draftKey="c1" />);
  expect(await screen.findByRole("status", { name: "Refinando prompt" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Cancelar refinamento" })).toBeEnabled();
  await act(async () => finish("Final A"));
  expect(await screen.findByRole("textbox", { name: "Mensagem" })).toHaveTextContent("Final A");
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "refine_chat_prompt")).toHaveLength(1);
});

it("stops refinement without losing the original and rejects its late result after a new edit", async () => {
  let finish!: (value: string) => void;
  vi.mocked(invoke).mockImplementation(async command => command === "refine_chat_prompt" ? new Promise<string>(resolve => { finish = resolve; }) : catalog);
  const drafts = new Map<string, ChatDraft>([["c1", { content: "Meu texto original" }]]);
  const user = userEvent.setup();
  render(<ChatComposer draftKey="c1" drafts={drafts} modelGroups={modelGroups} onSendMessage={vi.fn()} />);
  await user.click(await screen.findByRole("button", { name: "Refinar prompt" }));
  const stop = await screen.findByRole("button", { name: "Cancelar refinamento" });
  expect(stop).toHaveTextContent("");
  await user.click(stop);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  expect(field).toHaveTextContent("Meu texto original");
  expect(screen.getByRole("button", { name: "Refinar prompt" })).toBeEnabled();
  const request = vi.mocked(invoke).mock.calls.find(([command]) => command === "refine_chat_prompt")?.[1];
  const cancellation = vi.mocked(invoke).mock.calls.find(([command]) => command === "cancel_chat_prompt_refinement")?.[1];
  expect(cancellation).toEqual({ requestId: expect.any(String) });
  expect(request).toMatchObject(cancellation ?? {});
  await user.click(field);
  await user.keyboard(" ajustado");
  const edited = drafts.get("c1");
  expect(edited?.content).toContain("ajustado");
  await act(async () => finish("Resultado cancelado"));
  expect(field).toHaveTextContent(edited?.content.trim() ?? "");
  expect(drafts.get("c1")).toBe(edited);
});

it("preserves external draft additions made while refinement is pending", async () => {
  let finish!: (value: string) => void;
  vi.mocked(invoke).mockImplementation(async command => command === "refine_chat_prompt" ? new Promise<string>(resolve => { finish = resolve; }) : catalog);
  const drafts = new Map<string, ChatDraft>([["c1", { content: "Rascunho original" }]]);
  const props = { draftKey: "c1", drafts, modelGroups, onSendMessage: vi.fn() };
  const view = render(<ChatComposer {...props} />);
  const field = await screen.findByRole("textbox", { name: "Mensagem" });
  await userEvent.setup().click(screen.getByRole("button", { name: "Refinar prompt" }));
  await screen.findByRole("status", { name: "Refinando prompt" });
  view.rerender(<ChatComposer {...props} draftInsertion={{ id: "new-context", text: "Contexto adicionado" }} />);
  await act(async () => finish("Prompt anterior refinado"));
  expect(field).toHaveTextContent("Rascunho original");
  expect(field).toHaveTextContent("Contexto adicionado");
  expect(toast.info).toHaveBeenCalled();
});

it("keeps refinement disabled for an empty draft", async () => {
  render(<ChatComposer modelGroups={modelGroups} onSendMessage={vi.fn()} />);
  await screen.findByRole("textbox", { name: "Mensagem" });
  expect(screen.getByRole("button", { name: "Refinar prompt" })).toBeDisabled();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "refine_chat_prompt")).toBe(false);
});
