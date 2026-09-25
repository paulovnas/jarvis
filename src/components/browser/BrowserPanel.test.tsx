import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useBrowser } from "@/hooks/use-browser";
import type { BrowserSnapshot } from "@/core/browser";
import { FileWorkspace } from "@/components/files/FileWorkspace";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useState } from "react";
import { BrowserPanel } from "./BrowserPanel";
import type { BrowserController } from "@/hooks/use-browser";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
let stored: BrowserSnapshot;
afterEach(() => vi.restoreAllMocks());

function Workspace({ conversationId = "chat-1" }: { conversationId?: string }) {
  const browser = useBrowser(conversationId);
  const [draft, setDraft] = useState("");
  return <FileWorkspace browser={browser}><input aria-label="Rascunho" value={draft} onChange={event => setDraft(event.target.value)} /><Button onClick={() => void browser.open()}>Abrir navegador</Button></FileWorkspace>;
}
beforeEach(() => {
  stored = { tabs: [], activeId: null };
  mocked.mockReset().mockImplementation(async (command, args) => {
    if (command === "get_browser_tabs") return structuredClone(stored);
    if (command === "set_browser_viewport") return;
    if (command === "get_chat_attachment_image") return "data:image/png;base64,dGVzdA==";
    if (command === "browser_command") {
      const { request } = args as { request: { action: string; id?: string | null; url?: string } };
      if (request.action === "open") { const id = `tab-${stored.tabs.length + 1}`; stored.tabs.push({ id, conversationId: "chat-1", title: id, url: "about:blank", loading: false }); stored.activeId = id; }
      if (request.action === "select") stored.activeId = request.id ?? null;
      if (request.action === "close") { stored.tabs = stored.tabs.filter(t => t.id !== request.id); if (stored.activeId === request.id) stored.activeId = null; }
      if (request.action === "console") return { logs: [{ level: "error", text: "Erro de teste", time: 1 }] };
      if (request.action === "screenshot") return { attachment: { id: "capture-1" } };
      return {};
    }
    throw new Error(`Unexpected command: ${command}`);
  });
});

it("opens browser tabs beside permanent Chat, preserves drafts and closes an inactive tab", async () => {
  const user = userEvent.setup(); render(<Workspace />);
  await user.type(screen.getByRole("textbox", { name: "Rascunho" }), "Continuar tarefa");
  await user.click(screen.getByRole("button", { name: "Abrir navegador" }));
  await screen.findByRole("textbox", { name: "Endereço do navegador" });
  expect(screen.getByRole("tab", { name: "tab-1" })).toHaveAttribute("aria-selected", "true");
  expect(screen.queryByRole("button", { name: /Fechar.*Chat/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Nova aba do navegador" }));
  await screen.findByRole("tab", { name: "tab-2" });
  await user.click(screen.getByRole("button", { name: "Fechar navegador tab-1" }));
  await waitFor(() => expect(screen.queryByRole("tab", { name: "tab-1" })).not.toBeInTheDocument());
  expect(screen.getByRole("tab", { name: "tab-2" })).toHaveAttribute("aria-selected", "true");
  await user.click(screen.getByRole("tab", { name: "Chat" }));
  expect(screen.getByRole("textbox", { name: "Rascunho" })).toHaveValue("Continuar tarefa");
});

it("restores the selected browser tab and sends normalized URLs, console and capture requests", async () => {
  stored = { tabs: [{ id: "tab-1", conversationId: "chat-1", title: "Projeto", url: "http://localhost:3000/", loading: false }], activeId: "tab-1" };
  const user = userEvent.setup(); const first = render(<Workspace />);
  const address = await screen.findByRole("textbox", { name: "Endereço do navegador" });
  await user.clear(address); await user.type(address, "localhost:5173/teste{Enter}");
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("browser_command", { conversationId: "chat-1", request: { action: "navigate", id: "tab-1", url: "http://localhost:5173/teste" } }));
  await user.click(screen.getByRole("button", { name: "Mostrar console" }));
  expect(await screen.findByText("Erro de teste", { exact: false })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Capturar página" }));
  expect(await screen.findByRole("img", { name: "Captura da página aberta no navegador" })).toBeVisible();
  first.unmount(); await act(async () => {});
  render(<Workspace />);
  expect(await screen.findByRole("tab", { name: "Projeto" })).toHaveAttribute("aria-selected", "true");
});

it("keeps a native page mounted during navigation and hides it only while an overlay is open", async () => {
  const rect = { x: 210, y: 120, width: 710, height: 520 };
  const measure = Element.prototype.getBoundingClientRect;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    return this.getAttribute("aria-label") === "Página do navegador" ? DOMRect.fromRect(rect) : measure.call(this);
  });
  const tab = { id: "tab-1", conversationId: "chat-1", title: "Projeto", url: "http://localhost:3000/", loading: false };
  const browser: BrowserController = { conversationId: "chat-1", snapshot: { tabs: [tab], activeId: tab.id }, busy: false, loaded: true, open: vi.fn(), select: vi.fn(), command: vi.fn() };
  const viewports = () => mocked.mock.calls.filter(([command]) => command === "set_browser_viewport").map(([, args]) => args);
  const expected = (viewport: typeof rect | null) => ({ conversationId: "chat-1", id: tab.id, viewport });
  const { rerender } = render(<BrowserPanel browser={browser} tab={tab} />);
  await waitFor(() => expect(viewports()).toEqual([expected(rect)]));
  rerender(<BrowserPanel browser={browser} tab={{ ...tab, url: "http://localhost:3000/next" }} />);
  await act(async () => { window.dispatchEvent(new Event("resize")); await new Promise(resolve => requestAnimationFrame(resolve)); });
  expect(viewports()).toEqual([expected(rect)]);
  const menu = document.createElement("div"); menu.setAttribute("role", "menu");
  act(() => { document.body.append(menu); });
  await waitFor(() => expect(viewports()).toEqual([expected(rect), expected(null)]));
  act(() => { menu.remove(); });
  await waitFor(() => expect(viewports()).toEqual([expected(rect), expected(null), expected(rect)]));
  // The native page must follow the browser slot, not the full window, when
  // side panels resize or the console consumes vertical space.
  Object.assign(rect, { x: 260, width: 580, height: 344 });
  mocked.mockClear();
  await act(async () => { window.dispatchEvent(new Event("resize")); });
  await waitFor(() => expect(viewports()).toEqual([expected(rect)]));
});

it("keeps the browser visible when hovering its tab and toolbar tooltips", async () => {
  const rect = { x: 210, y: 120, width: 710, height: 520 };
  const measure = Element.prototype.getBoundingClientRect;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    return this.getAttribute("aria-label") === "Página do navegador" ? DOMRect.fromRect(rect) : measure.call(this);
  });
  const url = "http://localhost:5173/";
  stored = { tabs: [{ id: "tab-1", conversationId: "chat-1", title: "Projeto", url, loading: false }], activeId: "tab-1" };
  render(<TooltipProvider delay={0}><Workspace /></TooltipProvider>);
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("set_browser_viewport", { conversationId: "chat-1", id: "tab-1", viewport: rect }));
  mocked.mockClear();
  for (const [trigger, content] of [
    [screen.getByRole("tab", { name: "Projeto" }), url],
    [screen.getByRole("button", { name: "Recarregar página" }), "Recarregar"],
  ] as const) {
    fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
    fireEvent.mouseEnter(trigger);
    fireEvent.mouseMove(trigger);
    expect(await screen.findByRole("tooltip")).toHaveTextContent(content);
    // Let the overlay observer and its native viewport update finish.
    await act(async () => { await new Promise(resolve => requestAnimationFrame(resolve)); });
    expect(mocked.mock.calls.filter(([command]) => command === "set_browser_viewport")).toEqual([]);
    fireEvent.pointerLeave(trigger, { pointerType: "mouse" });
    fireEvent.mouseLeave(trigger);
    await waitFor(() => expect(screen.queryByRole("tooltip")).not.toBeInTheDocument());
  }
});
