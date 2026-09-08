import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ITerminalOptions } from "@xterm/xterm";
import type { ChatProcess } from "@/core/processes";
import { TerminalWorkspace } from "./TerminalWorkspace";
import { DesktopLayoutProvider } from "@/components/layout/DesktopLayoutProvider";
import { DEFAULT_DESKTOP_LAYOUT, type DesktopLayout } from "@/core/desktop-layout";

function TestWorkspace({ conversationId }: { conversationId?: string }) {
  return <TerminalWorkspace conversationId={conversationId}>{launcher => <><textarea aria-label="Mensagem" />{launcher}</>}</TerminalWorkspace>;
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
const renderer = vi.hoisted(() => ({ create: vi.fn(), open: vi.fn(), fit: vi.fn(), write: vi.fn(), dispose: vi.fn() }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() { renderer.fit(); } } }));
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    constructor(options: ITerminalOptions) { renderer.create(options); }
    cols = 100;
    rows = 24;
    loadAddon() {}
    open(element: HTMLElement) { renderer.open(element); }
    onData() { return { dispose() {} }; }
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

const process: ChatProcess = { id: "service", conversationId: "chat", title: "Vite", command: "bun run dev", cwd: "/project", pid: 123, startedAt: 1, endedAt: null, exitCode: null, status: "running" };

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

  it("separates section navigation from shell tabs and creates terminals after the last tab", async () => {
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
    const sections = screen.getByRole("tablist", { name: "Seções do painel de terminais" });
    const terminalSection = within(sections).getByRole("tab", { name: /^Terminais/ });
    expect(within(sections).getAllByRole("tab")).toHaveLength(2);
    const shells = screen.getByRole("tablist", { name: "Terminais abertos" });
    expect(within(shells).getAllByRole("tab")).toHaveLength(2);
    await user.click(within(shells).getByRole("tab", { name: "Terminal 2" }));
    terminalSection.focus();
    await user.keyboard("{ArrowRight}{Enter}");
    expect(within(sections).getByRole("tab", { name: "Processos" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("Nenhum processo gerenciado.")).toBeVisible();
    await user.keyboard("{ArrowLeft}{Enter}");
    expect(screen.getByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
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

  it("keeps managed process logs and stop confirmation in the terminal panel", async () => {
    let stopped = false;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_terminals") return [];
      if (command === "list_chat_processes") return [{ ...process, status: stopped ? "stopped" : "running" }];
      if (command === "read_chat_process") return { output: "Local: http://localhost:1420/" };
      if (command === "stop_chat_process") {
        stopped = true;
        return undefined;
      }
      return undefined;
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);

    await user.click(await screen.findByRole("button", { name: "1 processo monitorado" }));
    await user.click(screen.getByRole("tab", { name: /^Processos/ }));
    expect(screen.getByText("bun run dev")).toBeVisible();
    expect(invoke).not.toHaveBeenCalledWith("read_chat_process", expect.anything());
    await user.click(screen.getByRole("button", { name: "Saída" }));
    expect(await screen.findByLabelText("Saída de Vite")).toHaveTextContent("localhost:1420");
    await user.click(screen.getByRole("button", { name: "Parar Vite" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Parar Vite?");
    expect(invoke).not.toHaveBeenCalledWith("stop_chat_process", expect.anything());
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Parar Vite" }));
    await user.click(screen.getByRole("button", { name: "Parar processo" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("stop_chat_process", { conversationId: "chat", id: "service", confirmed: true }));
    await waitFor(() => expect(screen.getByText("Nenhum processo gerenciado.")).toBeVisible());
  });

  it.each(["failed", "exited"] as const)("keeps a %s process accessible until removal", async status => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_terminals") return [];
      if (command === "list_chat_processes") return [{ ...process, status, exitCode: status === "failed" ? 127 : 0 }];
      if (command === "read_chat_process") return { output: "/bin/bash: npm: No such file or directory" };
      if (command === "remove_chat_process") return undefined;
      return undefined;
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);

    await user.click(await screen.findByRole("button", { name: "1 processo monitorado" }));
    await user.click(screen.getByRole("tab", { name: /^Processos/ }));
    expect(screen.queryByRole("button", { name: "Parar Vite" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Saída" }));
    expect(await screen.findByLabelText("Saída de Vite")).toHaveTextContent("No such file or directory");
    await user.click(screen.getByRole("button", { name: "Remover Vite" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("remove_chat_process", { conversationId: "chat", id: "service" }));
    await waitFor(() => expect(screen.getByText("Nenhum processo gerenciado.")).toBeVisible());
    expect(invoke).not.toHaveBeenCalledWith("stop_chat_process", expect.anything());
  });

  it("keeps a failed process visible when removal fails and allows retry", async () => {
    let fail = true;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_terminals") return [];
      if (command === "list_chat_processes") return [{ ...process, status: "failed" }];
      if (command === "remove_chat_process" && fail) throw { message: "Falha ao remover." };
      return undefined;
    });
    const user = userEvent.setup();
    render(<TestWorkspace conversationId="chat" />);

    await user.click(await screen.findByRole("button", { name: "1 processo monitorado" }));
    await user.click(screen.getByRole("tab", { name: /^Processos/ }));
    await user.click(screen.getByRole("button", { name: "Remover Vite" }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Falha ao remover."));
    expect(screen.getByText("Falhou")).toBeVisible();
    expect(screen.getByRole("button", { name: "Remover Vite" })).toBeEnabled();
    fail = false;
    await user.click(screen.getByRole("button", { name: "Remover Vite" }));
    await waitFor(() => expect(screen.getByText("Nenhum processo gerenciado.")).toBeVisible());
  });

  it("does not show another conversation's managed processes after navigation", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_chat_terminals") return [];
      if (command === "list_chat_processes") {
        return args && typeof args === "object" && "conversationId" in args && args.conversationId === "chat"
          ? [{ ...process, status: "failed" }]
          : [];
      }
      return undefined;
    });
    const user = userEvent.setup();
    const view = render(<TestWorkspace conversationId="chat" />);

    await user.click(await screen.findByRole("button", { name: "1 processo monitorado" }));
    await user.click(screen.getByRole("tab", { name: /^Processos/ }));
    expect(screen.getByText("Vite")).toBeVisible();
    view.rerender(<TestWorkspace conversationId="other" />);
    expect(screen.queryByText("Vite")).not.toBeInTheDocument();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("list_chat_processes", { conversationId: "other" }));
    expect(screen.queryByRole("region", { name: "Painel de terminais" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Abrir terminais" }));
    await waitFor(() => expect(screen.getByText("Nenhum processo gerenciado.")).toBeVisible());
  });
});
