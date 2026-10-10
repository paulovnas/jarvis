import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import { within } from "@testing-library/dom";
import "@testing-library/jest-dom/vitest";

type Status = { conversationId: string; tabId: string; actionId: string | null; revision: number; phase: string; state: string; error: string | null; model?: string; provider?: string; step?: number; activity?: string; elapsedMs?: number; queueCount?: number; feedback?: { requestId: string; error: string | null } };
type Intent = { requestId: string; conversationId: string; tabId: string; actionId: string; action: "cancel" | "resume"; revision: number };
type Adapter = { update: (status: Status) => void; drain: () => Intent[]; dispose: () => void };
const source = readFileSync("src-tauri/resources/impeccable/jarvis-live-adapter.js", "utf8");
const disposals: (() => void)[] = [];
afterEach(() => { disposals.splice(0).forEach(dispose => dispose()); document.body.replaceChildren(); });
function fixture() {
  const host = document.createElement("div"); document.body.append(host);
  const root = host.attachShadow({ mode: "open" });
  const bar = document.createElement("div"); bar.id = "impeccable-live-global-bar";
  const picker = document.createElement("button"); picker.textContent = "Escolher elemento"; picker.id = "impeccable-live-pick-toggle";
  const select = vi.fn(); picker.addEventListener("click", select);
  const send = document.createElement("button"); send.id = "impeccable-live-page-chat-send";
  const shader = document.createElement("div"); shader.id = "impeccable-live-shader";
  bar.append(picker, send); root.append(bar, shader);
  const target: Record<string, unknown> = { __IMPECCABLE_LIVE_CHROME_CORE__: { getById: (id: string) => root.querySelector(`#${id}`), root, append: (node: Node) => root.append(node) } };
  new Function("window", "document", "MutationObserver", "crypto", source)(target, document, MutationObserver, crypto);
  const adapter = target.__JARVIS_IMPECCABLE_LIVE__ as Adapter;
  disposals.push(() => adapter.dispose());
  const status: Status = { conversationId: "c1", tabId: "b1", actionId: "a1", revision: 1, phase: "waiting", state: "ready", error: null };
  return { adapter, status, bar, target, picker, select, shader };
}

it("shows native waiting/progress in the actual shadow toolbar and retains element selection", () => {
  const { adapter, status, bar, picker, select, target } = fixture();
  adapter.update({ ...status, model: "current-model", provider: "OpenAI", step: 3, activity: "Ajustando a posição da versão", elapsedMs: 64000, queueCount: 1 });
  const ui = within(bar);
  expect(ui.getByRole("status")).toHaveTextContent("Aguardando o agente ficar disponível");
  expect(ui.getByRole("status")).toHaveTextContent("OpenAI · current-model · Etapa 3 · 1 min 4 s · 1 na fila");
  expect(ui.getByRole("status")).toHaveTextContent("Ajustando a posição da versão");
  picker.click(); expect(select).toHaveBeenCalledOnce();
  adapter.update({ ...status, revision: 2, phase: "running" });
  expect(ui.getByRole("status")).toHaveTextContent("Designer trabalhando");
  expect(target.__TAURI_INTERNALS__).toBeUndefined();
});

it("queues one scoped cancel intent and renders recoverable errors without modifying variants", () => {
  const { adapter, status, bar, shader } = fixture();
  const variant = document.createElement("div"); variant.textContent = "Minha variante"; document.body.append(variant);
  adapter.update({ ...status, phase: "running" });
  expect(shader.style.visibility).toBe("");
  within(bar).getByRole("button", { name: "Cancelar alteração" }).click();
  const requests = adapter.drain(); expect(requests).toHaveLength(1);
  expect(requests[0]).toMatchObject({ action: "cancel", conversationId: "c1", tabId: "b1", actionId: "a1", revision: 1 });
  expect(within(bar).getByRole("button", { name: "Cancelar alteração" })).toBeDisabled();
  adapter.update({ ...status, revision: 2, phase: "blocked", state: "setup", error: "Alteração interrompida. Retome quando quiser.", feedback: { requestId: requests[0].requestId, error: null } });
  expect(within(bar).getByRole("status")).toHaveTextContent("Alteração interrompida");
  expect(shader.style.visibility).toBe("hidden");
  expect(shader.isConnected).toBe(true);
  expect(within(bar).getByRole("button", { name: "Retomar" })).toBeEnabled();
  expect(variant).toHaveTextContent("Minha variante");
  adapter.update({ ...status, revision: 3, phase: "running" });
  expect(shader.style.visibility).toBe("");
});

it("retains real errors, rejects stale foreign status, and confirms completion only from native idle", () => {
  const { adapter, status, bar } = fixture();
  adapter.update({ ...status, revision: 4, phase: "blocked", error: "O provedor não respondeu." });
  adapter.update({ ...status, revision: 3, phase: "running" });
  adapter.update({ ...status, conversationId: "other", revision: 8, phase: "running" });
  expect(within(bar).getByRole("status")).toHaveTextContent("O provedor não respondeu");
  within(bar).getByRole("button", { name: "Retomar" }).click();
  const [request] = adapter.drain();
  expect(request.action).toBe("resume");
  adapter.update({ ...status, revision: 5, phase: "blocked", feedback: { requestId: request.requestId, error: "Escolha um modelo no chat." } });
  expect(within(bar).getByRole("status")).toHaveTextContent("Escolha um modelo no chat");
  adapter.update({ ...status, revision: 6, phase: "running" });
  expect(within(bar).getByRole("status")).not.toHaveTextContent("concluída");
  adapter.update({ ...status, revision: 7, phase: "idle", actionId: null, feedback: { requestId: request.requestId, error: null } });
  expect(within(bar).getByRole("status")).toHaveTextContent("Alteração concluída. Revise o resultado.");
});

it("remounts idempotently after toolbar replacement and removes only its own chrome on stop", () => {
  const { adapter, status, bar, target } = fixture();
  adapter.update(status); adapter.update(status);
  expect(bar.querySelectorAll('[role="status"]')).toHaveLength(1);
  const newBar = bar.cloneNode(false) as HTMLElement;
  bar.replaceWith(newBar);
  adapter.update({ ...status, revision: 2 });
  expect(within(newBar).getByRole("status")).toHaveTextContent("Aguardando o agente ficar disponível");
  new Function("window", "document", "MutationObserver", "crypto", source)(target, document, MutationObserver, crypto);
  expect(newBar.querySelectorAll('[role="status"]')).toHaveLength(1);
  adapter.update({ ...status, revision: 3, state: "off" });
  expect(within(newBar).queryByRole("status")).not.toBeInTheDocument();
});
