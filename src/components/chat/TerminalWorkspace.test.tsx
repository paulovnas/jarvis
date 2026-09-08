import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ITerminalOptions } from "@xterm/xterm";
import type { ChatTerminal } from "@/core/terminals";
import { TerminalWorkspace } from "./TerminalWorkspace";
import { DesktopLayoutProvider } from "@/components/layout/DesktopLayoutProvider";
import { DEFAULT_DESKTOP_LAYOUT, type DesktopLayout } from "@/core/desktop-layout";

function TestWorkspace({ conversationId }: { conversationId?: string }) {
  return <TerminalWorkspace conversationId={conversationId}>{launcher => <><textarea aria-label="Mensagem" />{launcher}</>}</TerminalWorkspace>;
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
const renderer = vi.hoisted(() => ({ create: vi.fn(), open: vi.fn(), fit: vi.fn(), write: vi.fn(), dispose: vi.fn(), input: vi.fn<(data: string) => void>() as (data: string) => void }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() { renderer.fit(); } } }));
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    constructor(options: ITerminalOptions) { renderer.create(options); }
    cols = 100;
    rows = 24;
    loadAddon() {}
    open(element: HTMLElement) { renderer.open(element); }
    onData(callback: (data: string) => void) { renderer.input = callback; return { dispose() { renderer.input = () => {}; } }; }
    write(data: string) { renderer.write(data); }
    reset() {}
    focus() {}
    dispose() { renderer.dispose(); }
  },
}));

const terminals = [
  { id: "terminal-1", conversationId: "chat", title: "Terminal 1", cwd: "/project", pid: 1, startedAt: 1, endedAt: null, exitCode: null, status: "running", origin: "user" },
  { id: "terminal-2", conversationId: "chat", title: "Terminal 2", cwd: "/project", pid: 2, startedAt: 2, endedAt: null, exitCode: null, status: "running", origin: "user" },
] as const;

const service: ChatTerminal = { id: "service", origin: "agent", conversationId: "chat", title: "Vite", command: "bun run dev", cwd: "/project", pid: 123, startedAt: 1, endedAt: null, exitCode: null, status: "running" };

describe("Integrated terminals", () => {
  const originalFonts = Object.getOwnPropertyDescriptor(document, "fonts");

  afterEach(() => {
    vi.restoreAllMocks();
    if (originalFonts) Object.defineProperty(document, "fonts", originalFonts);
    else Reflect.deleteProperty(document, "fonts");
  });

  beforeEach(() => {
    vi.clearAllMocks();
    let created = 0;
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_terminals") return [];
      if (command === "list_chat_processes") return [];
      if (command === "create_chat_terminal") return terminals[created++];
      if (command === "read_chat_terminal") {
        return { terminal: terminals[Math.min(created, 1)], output: "", revision: 0, truncated: false };
      }
      return undefined;
    });
  });

  it("docks below the chat, keeps the draft editable, and collapses without stopping terminals", async () => {
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    const message = screen.getByRole("textbox", { name: "Mensagem" });
    await user.type(message, "Continuar revisão");
    await user.click(screen.getByRole("button", { name: "Abrir terminais" }));
    const panel = await screen.findByRole("region", { name: "Painel de terminais" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(message.compareDocumentPosition(panel) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByRole("separator", { name: "Redimensionar painel de terminais" })).toHaveAttribute("aria-orientation", "horizontal");
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    await screen.findByRole("application");
    expect(message.isConnected).toBe(true);
    expect(screen.getByRole("textbox", { name: "Mensagem" })).toBe(message);
    message.focus();
    expect(message).toHaveFocus();
    await user.keyboard(" com terminal aberto");
    expect(message).toHaveValue("Continuar revisão com terminal aberto");
    await user.click(screen.getByRole("button", { name: "Recolher painel de terminais" }));
    expect(screen.queryByRole("region", { name: "Painel de terminais" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1 terminal aberto" })).toHaveFocus();
    expect(invoke).not.toHaveBeenCalledWith("close_chat_terminal", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("stop_chat_process", expect.anything());
    await user.click(screen.getByRole("button", { name: "1 terminal aberto" }));
    expect(await screen.findByRole("tab", { name: "Terminal 1" })).toBeVisible();
    expect(message).toHaveValue("Continuar revisão com terminal aberto");
  });

  it("restores each chat's open panel, size, and selected terminal after navigation and restart", async () => {
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
    let saved: DesktopLayout = { ...DEFAULT_DESKTOP_LAYOUT, terminalPanels: { chat: { open: false, size: 57, activeTerminalId: null } } };
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_desktop_layout") return saved;
      if (command === "save_desktop_layout") saved = (args as { layout: DesktopLayout }).layout;
      if (command === "list_chat_terminals") return (args as { conversationId: string }).conversationId === "chat" ? terminals : [];
      if (command === "list_chat_processes") return [];
      if (command === "read_chat_terminal") return { terminal: terminals[1], output: "", revision: 0, truncated: false };
    });
    const user = userEvent.setup();
    const workspace = (id: string) => <DesktopLayoutProvider><TestWorkspace key={id} conversationId={id} /></DesktopLayoutProvider>;
    const view = render(workspace("chat"));
    await user.click(await screen.findByRole("button", { name: "2 terminais abertos" }));
    await user.click(await screen.findByRole("tab", { name: "Terminal 2" }));
    await waitFor(() => expect(saved.terminalPanels.chat).toEqual({ open: true, size: 57, activeTerminalId: "terminal-2" }));
    view.rerender(workspace("other-chat"));
    expect(screen.queryByRole("region", { name: "Painel de terminais" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Abrir terminais" }));
    view.rerender(workspace("chat"));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(screen.getByRole("separator", { name: "Redimensionar painel de terminais" })).toHaveAttribute("aria-valuenow", "43"));
    view.unmount();
    render(workspace("chat"));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("button", { name: "Recolher painel de terminais" }));
    await waitFor(() => expect(saved.terminalPanels.chat.open).toBe(false));
    expect(saved.terminalPanels.chat.size).toBe(57);
    expect(saved.terminalPanels["other-chat"].open).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("close_chat_terminal", expect.anything());
  });

  it("saves a user resize and restores it after reopening the panel", async () => {
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
    let saved = DEFAULT_DESKTOP_LAYOUT;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_desktop_layout") return saved;
      if (command === "save_desktop_layout") saved = (args as { layout: DesktopLayout }).layout;
      if (command === "list_chat_terminals" || command === "list_chat_processes") return [];
    });
    render(<DesktopLayoutProvider><TestWorkspace conversationId="chat" /></DesktopLayoutProvider>);
    fireEvent.click(await screen.findByRole("button", { name: "Abrir terminais" }));
    const handle = screen.getByRole("separator", { name: "Redimensionar painel de terminais" });
    await waitFor(() => expect(handle).toHaveAttribute("aria-valuenow", "60"));
    fireEvent.keyDown(handle, { key: "ArrowUp" });
    await waitFor(() => expect(saved.terminalPanels.chat.size).toBeGreaterThan(40));
    const size = saved.terminalPanels.chat.size;
    fireEvent.click(screen.getByRole("button", { name: "Recolher painel de terminais" }));
    fireEvent.click(screen.getByRole("button", { name: "Abrir terminais" }));
    await waitFor(() => expect(screen.getByRole("separator", { name: "Redimensionar painel de terminais" })).toHaveAttribute("aria-valuenow", String(100 - size)));
  });

  it("waits for the resolved monospace font before measuring and fitting the terminal", async () => {
    const fontFamily = '"JetBrains Mono", Consolas, monospace';
    const computedStyle = window.getComputedStyle.bind(window);
    vi.spyOn(window, "getComputedStyle").mockImplementation(element => {
      const styles = computedStyle(element);
      if (element.getAttribute("role") === "application") {
        Object.defineProperty(styles, "fontFamily", { value: fontFamily });
      }
      return styles;
    });
    let resolveFont: (fonts: FontFace[]) => void = () => {};
    const loadFont = vi.fn(() => new Promise<FontFace[]>(resolve => { resolveFont = resolve; }));
    Object.defineProperty(document, "fonts", { configurable: true, value: { load: loadFont } });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    await user.click(screen.getByRole("button", { name: "Abrir terminais" }));
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    const surface = await screen.findByRole("application", { name: "Terminal Terminal 1" });
    Object.defineProperties(surface, { clientWidth: { value: 1_080 }, clientHeight: { value: 580 } });

    expect(loadFont).toHaveBeenCalledWith(`13px ${fontFamily}`);
    expect(renderer.open).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("resize_chat_terminal", expect.anything());
    await act(async () => resolveFont([]));

    await waitFor(() => expect(renderer.open).toHaveBeenCalledWith(surface));
    expect(renderer.create).toHaveBeenCalledWith(expect.objectContaining({ fontFamily, fontSize: 13, letterSpacing: 0 }));
    expect(renderer.fit).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("resize_chat_terminal", { conversationId: "chat", id: "terminal-1", rows: 24, cols: 100 });
    expect(renderer.write).toHaveBeenCalled();
  });

  it("opens the terminal even when the bundled font cannot be loaded", async () => {
    Object.defineProperty(document, "fonts", { configurable: true, value: { load: vi.fn().mockRejectedValue(new Error("Font unavailable")) } });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    await user.click(screen.getByRole("button", { name: "Abrir terminais" }));
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    await waitFor(() => expect(renderer.open).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("read_chat_terminal", { conversationId: "chat", id: "terminal-1" });
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("does not open or resize a terminal after closing while its font loads", async () => {
    let resolveFont: (fonts: FontFace[]) => void = () => {};
    Object.defineProperty(document, "fonts", { configurable: true, value: { load: () => new Promise<FontFace[]>(resolve => { resolveFont = resolve; }) } });
    const user = userEvent.setup();
    const view = render(<TestWorkspace conversationId="chat" />);
    await user.click(screen.getByRole("button", { name: "Abrir terminais" }));
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    await screen.findByRole("application");
    view.unmount();
    await act(async () => resolveFont([]));

    expect(renderer.dispose).toHaveBeenCalled();
    expect(renderer.open).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("read_chat_terminal", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("resize_chat_terminal", expect.anything());
  });

  it.each([String.raw`C:\Users\pauli\Documents\projetos\ação [teste]`, String.raw`\\servidor\projetos\Jarvis`, "/Users/pauli/projects/Jarvis"])("shows the full project path in the footer tooltip: %s", async cwd => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_terminals") return [{ ...terminals[0], cwd }];
      if (command === "list_chat_processes") return [];
      if (command === "read_chat_terminal") return { terminal: { ...terminals[0], cwd }, output: "", revision: 0, truncated: false };
      return undefined;
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "1 terminal aberto" }));
    expect(await screen.findByTitle(cwd)).toHaveTextContent(cwd);
    expect(screen.getByText("Em execução")).toBeVisible();
  });

  it("keeps the launcher visible, creates tabs, renames them, and requires confirmation before closing", async () => {
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);

    const launcher = screen.getByRole("button", { name: "Abrir terminais" });
    expect(within(launcher).queryByText(/\d/)).not.toBeInTheDocument();
    await user.click(launcher);
    expect(await screen.findByText("Nenhum terminal aberto")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Novo Terminal" }));
    await screen.findByRole("tab", { name: "Terminal 1" });
    await user.click(screen.getByRole("button", { name: "Novo terminal" }));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("create_chat_terminal", { conversationId: "chat" });

    await user.click(screen.getByRole("button", { name: "Recolher painel de terminais" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Painel de terminais" })).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "2 terminais abertos" }));
    const firstTab = await screen.findByRole("tab", { name: "Terminal 1" });
    fireEvent.contextMenu(firstTab);
    await user.click(await screen.findByRole("menuitem", { name: "Renomear" }));
    const name = await screen.findByRole("textbox", { name: "Novo nome para Terminal 1" });
    await user.clear(name);
    await user.type(name, "Console");
    await user.click(screen.getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("rename_chat_terminal", { conversationId: "chat", id: "terminal-1", title: "Console" }));

    await user.click(screen.getByRole("button", { name: "Fechar Console" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Fechar Console?");
    expect(invoke).not.toHaveBeenCalledWith("close_chat_terminal", expect.anything());
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("close_chat_terminal", { conversationId: "chat", id: "terminal-1", confirmed: true }));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
  });

  it("shows one terminal tab strip and creates terminals after the last tab", async () => {
    const third = { ...terminals[0], id: "terminal-3", title: "Terminal 3" };
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_chat_terminals") return terminals;
      if (command === "list_chat_processes") return [];
      if (command === "create_chat_terminal") return third;
      if (command === "read_chat_terminal") return { terminal: (args as { id: string }).id === third.id ? third : terminals[0], output: "", revision: 0, truncated: false };
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "2 terminais abertos" }));
    expect(screen.queryByRole("tab", { name: "Processos" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("tablist")).toHaveLength(1);
    const shells = screen.getByRole("tablist", { name: "Terminais abertos" });
    await user.click(within(shells).getByRole("tab", { name: "Terminal 2" }));
    const strip = screen.getByRole("group", { name: "Abas dos terminais" });
    const add = within(strip).getByRole("button", { name: "Novo terminal" });
    const last = within(strip).getByRole("tab", { name: "Terminal 2" });
    expect(last.compareDocumentPosition(add) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.click(add);
    expect(await screen.findByRole("tab", { name: "Terminal 3" })).toHaveAttribute("aria-selected", "true");
    const openTabs = within(screen.getByRole("tablist", { name: "Terminais abertos" })).getAllByRole("tab");
    expect(openTabs[openTabs.length - 1]).toHaveTextContent("Terminal 3");
    expect(screen.getByRole("tab", { name: "Terminal 3" }).compareDocumentPosition(add) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it.each([0, 1])("confirms closing terminal %i immediately, cancels, and retries without refreshing the panel", async index => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_chat_terminals") return terminals;
      if (command === "list_chat_processes") return [];
      if (command === "read_chat_terminal") return { terminal: terminals.find(terminal => terminal.id === (args as { id: string }).id), output: "", revision: 0, truncated: false };
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "2 terminais abertos" }));
    const selected = screen.getByRole("tab", { name: "Terminal 1" });
    const target = terminals[index];
    const close = screen.getByRole("button", { name: `Fechar ${target.title}` });
    expect(close).toHaveAttribute("aria-haspopup", "dialog");

    await user.click(close);
    expect(screen.getByRole("alertdialog")).toHaveTextContent(`Fechar ${target.title}?`);
    expect(selected).toHaveAttribute("aria-selected", "true");
    expect(invoke).not.toHaveBeenCalledWith("close_chat_terminal", expect.anything());
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(selected).toHaveAttribute("aria-selected", "true");

    await user.click(close);
    expect(screen.getByRole("alertdialog")).toHaveTextContent(`Fechar ${target.title}?`);
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(invoke).toHaveBeenCalledWith("close_chat_terminal", { conversationId: "chat", id: target.id, confirmed: true });
    expect(screen.queryByRole("tab", { name: target.title })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: terminals[1 - index].title })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("region", { name: "Painel de terminais" })).toBeVisible();
  });

  it("keeps the requested terminal and confirmation available after a close failure", async () => {
    let fail = true;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_chat_terminals") return terminals;
      if (command === "list_chat_processes") return [];
      if (command === "read_chat_terminal") return { terminal: terminals.find(terminal => terminal.id === (args as { id: string }).id), output: "", revision: 0, truncated: false };
      if (command === "close_chat_terminal" && fail) throw { message: "Falha ao fechar terminal." };
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "2 terminais abertos" }));
    await user.click(screen.getByRole("button", { name: "Fechar Terminal 2" }));
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Falha ao fechar terminal."));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Fechar Terminal 2?");
    expect(screen.getByRole("button", { name: "Fechar terminal" })).toBeEnabled();
    fail = false;
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(screen.queryByRole("tab", { name: "Terminal 2" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Terminal 1" })).toHaveAttribute("aria-selected", "true");
  });


  it.each(["running", "failed", "exited"] as const)("shows a %s service as a terminal with logs and confirmed closure", async status => {
    const item = { ...service, status };
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_chat_terminals") return [...terminals, item];
      if (command === "read_chat_terminal") return { terminal: (args as {id: string}).id === item.id ? item : terminals[0], output: "Local: http://localhost:1420/", revision: 1, truncated: false };
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "3 terminais abertos" }));
    await user.click(screen.getByRole("tab", { name: "Vite" }));
    await waitFor(() => expect(renderer.write).toHaveBeenCalledWith("Local: http://localhost:1420/"));
    expect(screen.getByRole("application", { name: "Terminal Vite" })).toBeVisible();
    expect(screen.queryByRole("tab", { name: "Processos" })).not.toBeInTheDocument();
    if (status === "running") {
      act(() => renderer.input("r\r"));
      await waitFor(() => expect(invoke).toHaveBeenCalledWith("write_chat_terminal", { conversationId: "chat", id: "service", input: "r\r" }));
    }
    await user.click(screen.getByRole("button", { name: "Fechar Vite" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Fechar Vite?");
    expect(invoke).not.toHaveBeenCalledWith("close_chat_terminal", expect.anything());
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("close_chat_terminal", { conversationId: "chat", id: "service", confirmed: true }));
    expect(screen.queryByRole("tab", { name: "Vite" })).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("list_chat_processes", expect.anything());
  });

  it("does not leak service terminals across conversations", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_chat_terminals") return (args as { conversationId: string }).conversationId === "chat" ? [service] : [];
      if (command === "read_chat_terminal") return { terminal: service, output: "", revision: 0, truncated: false };
    });
    const user = userEvent.setup();
    const view = render(<TestWorkspace conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "1 terminal aberto" }));
    expect(screen.getByRole("tab", { name: "Vite" })).toBeVisible();
    view.rerender(<TestWorkspace conversationId="other" />);
    expect(screen.queryByRole("tab", { name: "Vite" })).not.toBeInTheDocument();
    await screen.findByRole("button", { name: "Abrir terminais" });
    expect(invoke).not.toHaveBeenCalledWith("close_chat_terminal", expect.anything());
  });
});
