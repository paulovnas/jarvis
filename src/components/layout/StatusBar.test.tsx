import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ClaudeRuntime, MuseRuntime } from "@/core/executors";
import type { ProviderAccount } from "@/core/provider-accounts";
import type { AccountUsage } from "@/core/provider-usage";
import { StatusBar } from "./StatusBar";
import { ollamaAccountFixture } from "@/test/custom-provider-fixtures";

const runtimes = vi.hoisted(() => ({ claude: null as ClaudeRuntime | null, muse: null as MuseRuntime | null }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/hooks/use-claude-runtime", () => ({ useClaudeRuntime: () => ({ data: runtimes.claude }) }));
vi.mock("@/hooks/use-muse-runtime", () => ({ useMuseRuntime: () => ({ data: runtimes.muse }) }));
vi.mock("../remote/RemoteAccess", () => ({ RemoteAccess: () => <button aria-label="Modo remoto">Remoto</button> }));
vi.mock("./AppUpdate", () => ({ AppUpdate: () => <button aria-label="Versão">Versão</button> }));
vi.mock("./ResourceUpdates", () => ({ ResourceUpdates: () => null }));

const account: ProviderAccount = { alias: "codex", displayName: "Conta principal", providerKind: "openai-codex", enabled: true, createdAt: 1, email: "codex@example.test", accountType: "personal", models: [], modelsAvailable: true };
const runtime: ClaudeRuntime = { installed: true, authenticated: true, version: "2", error: null, models: [], preferences: { enabled: true, showUsage: true, disabledModels: [] } };
const report = (alias: string): AccountUsage => ({ alias, fetchedAt: Date.now(), email: `${alias}@example.test`, plan: "pro", error: null, resetCredits: null, windows: alias === "router" ? [] : [{ id: "weekly", group: alias === "codex" ? "Codex" : alias, thirdParty: false, label: "7d", durationSeconds: 604_800, remainingPercent: 36, resetsAt: Date.now() + 86_400_000 }] });
const originalResizeObserver = window.ResizeObserver;
const originalBounds = HTMLElement.prototype.getBoundingClientRect;
let availableWidth = 600;
let contentWidth = 600;
const observers = new Map<ResizeObserver, { callback: ResizeObserverCallback; elements: Set<Element> }>();

beforeEach(() => {
  runtimes.claude = null;
  runtimes.muse = null;
  availableWidth = 600;
  contentWidth = 600;
  observers.clear();
  vi.mocked(invoke).mockReset().mockImplementation((command, args) => Promise.resolve(report(command === "get_claude_usage" ? "Claude Code" : command === "get_muse_usage" ? "Muse Code" : (args as { alias: string }).alias)));
  window.ResizeObserver = class implements ResizeObserver {
    constructor(callback: ResizeObserverCallback) { observers.set(this, { callback, elements: new Set() }); }
    observe(element: Element) { observers.get(this)?.elements.add(element); }
    unobserve(element: Element) { observers.get(this)?.elements.delete(element); }
    disconnect() { observers.delete(this); }
  };
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.getAttribute("role") === "group" && this.getAttribute("aria-label") === "Limites dos provedores") return new DOMRect(0, 0, availableWidth, 28);
    if (this.parentElement?.getAttribute("role") === "group" && this.parentElement.getAttribute("aria-label") === "Limites dos provedores" && this === this.parentElement.firstElementChild) return new DOMRect(0, 0, contentWidth, 24);
    return originalBounds.call(this);
  });
});

afterEach(() => { window.ResizeObserver = originalResizeObserver; vi.restoreAllMocks(); vi.useRealTimers(); });

function resizeLimits(target: "available" | "content") {
  const limits = screen.getByRole("group", { name: "Limites dos provedores" });
  const element = target === "available" ? limits : limits.firstElementChild;
  act(() => {
    for (const [observer, { callback, elements }] of observers) if (element && elements.has(element)) callback([], observer);
  });
}

it.each([true, false])("does not query or display Ollama quotas with saved usage visibility %s", async showUsage => {
  const ollama = { ...ollamaAccountFixture(), showUsage, usageAlert: { window: "weekly" as const, remainingPercent: 20 } };
  render(<StatusBar accounts={[account, ollama]} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Limites de Conta principal" })).toHaveTextContent("36%"));
  expect(screen.queryByRole("button", { name: "Limites de Ollama" })).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalledWith("get_provider_usage", { alias: ollama.alias });
});
it("coloca o acesso remoto entre configurações e versão", async () => {
  render(<StatusBar onOpenSettings={() => {}} />);
  const remote = await screen.findByRole("button", { name: "Modo remoto" });
  const version = await screen.findByRole("button", { name: "Versão" });
  expect(screen.getByRole("button", { name: "Configurações" }).compareDocumentPosition(remote) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(remote.compareDocumentPosition(version) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("mostra a hora local, atualiza na virada do minuto e limpa o timer", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2026, 8, 5, 23, 59, 58));
  const { unmount } = render(<StatusBar />);
  expect(screen.getByLabelText("Hora atual")).toHaveTextContent("23:59");
  act(() => vi.advanceTimersByTime(2000));
  expect(screen.getByLabelText("Hora atual")).toHaveTextContent("00:00");
  act(() => window.dispatchEvent(new Event("focus")));
  expect(vi.getTimerCount()).toBe(1);
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});

it("recolhe somente quando a faixa completa excede o espaço e a restaura ao caber", async () => {
  const user = userEvent.setup();
  render(<StatusBar accounts={[account]} />);
  const provider = await screen.findByRole("button", { name: "Limites de Conta principal" });
  await waitFor(() => expect(provider).toHaveTextContent("36%"));
  expect(screen.queryByRole("button", { name: "Limites" })).not.toBeInTheDocument();
  availableWidth = 599;
  resizeLimits("available");
  const limits = screen.getByRole("button", { name: "Limites" });
  expect(screen.queryByRole("button", { name: "Limites de Conta principal" })).not.toBeInTheDocument();
  const hiddenProvider = screen.getByRole("button", { name: "Limites de Conta principal", hidden: true });
  expect(hiddenProvider.closest("[inert]")).toHaveAttribute("aria-hidden", "true");
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_provider_usage", { alias: "codex" });
  expect(limits.parentElement?.nextElementSibling).toBe(screen.getByLabelText("Hora atual"));
  await user.click(limits);
  expect(await screen.findByRole("dialog", { name: "Limites dos provedores" })).toBeVisible();
  availableWidth = 600;
  resizeLimits("available");
  expect(screen.queryByRole("button", { name: "Limites" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Limites de Conta principal" })).toHaveTextContent("36%");
  expect(screen.queryByRole("dialog", { name: "Limites dos provedores" })).not.toBeInTheDocument();
});

it.each(["click", "hover"] as const)("abre todos os limites visíveis com %s, incluindo Claude e Muse", async action => {
  const user = userEvent.setup();
  runtimes.claude = runtime;
  runtimes.muse = runtime;
  availableWidth = 500;
  const go = { ...account, alias: "go", displayName: "Conta Go", providerKind: "opencode-go", email: "go@example.test" };
  const router = { ...account, alias: "router", displayName: "OpenRouter", providerKind: "openrouter", showUsage: true, email: "router@example.test" };
  render(<StatusBar accounts={[account, go, router, { ...account, alias: "oculta", showUsage: false }, { ...account, alias: "inativa", enabled: false }, { ...account, alias: "custom", providerKind: "custom" }]} />);
  const limits = screen.getByRole("button", { name: "Limites" });
  await user[action](limits);
  const details = await screen.findByRole("dialog", { name: "Limites dos provedores" });
  for (const email of ["codex@example.test", "go@example.test", "router@example.test", "Claude Code@example.test", "Muse Code@example.test"]) expect(await within(details).findByText(email)).toBeVisible();
  expect(within(details).getAllByRole("progressbar")).toHaveLength(4);
  expect(within(details).queryByText(/oculta|inativa|custom/)).not.toBeInTheDocument();
  expect(within(details).queryByRole("button", { name: /^Limites de / })).not.toBeInTheDocument();
});

it("reavalia a largura dos limites quando os dados ou provedores mudam sem redimensionar a janela", async () => {
  contentWidth = 400;
  const { rerender } = render(<StatusBar accounts={[account]} />);
  await screen.findByRole("button", { name: "Limites de Conta principal" });
  contentWidth = 700;
  runtimes.claude = runtime;
  rerender(<StatusBar accounts={[account]} />);
  resizeLimits("content");
  expect(screen.getByRole("button", { name: "Limites" })).toBeVisible();
  contentWidth = 300;
  runtimes.claude = null;
  rerender(<StatusBar accounts={[]} />);
  resizeLimits("content");
  expect(screen.queryByRole("button", { name: "Limites" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Limites de Conta principal" })).not.toBeInTheDocument();
});

it("fecha os detalhes individuais quando a faixa é recolhida", async () => {
  const user = userEvent.setup();
  render(<StatusBar accounts={[account]} />);
  await user.click(await screen.findByRole("button", { name: "Limites de Conta principal" }));
  expect(await screen.findByRole("dialog", { name: "Limites de Conta principal" })).toBeVisible();
  availableWidth = 599;
  resizeLimits("available");
  expect(screen.getByRole("button", { name: "Limites" })).toBeVisible();
  expect(screen.queryByRole("dialog", { name: "Limites de Conta principal" })).not.toBeInTheDocument();
});
