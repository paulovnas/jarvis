import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ITerminalOptions } from "@xterm/xterm";
import type { ProjectTerminal } from "@/core/terminals";
import { TerminalWorkspace } from "./TerminalWorkspace";
import { DesktopLayoutProvider } from "@/components/layout/DesktopLayoutProvider";
import { DEFAULT_DESKTOP_LAYOUT, type DesktopLayout } from "@/core/desktop-layout";
import { terminalFontFamily, type SystemSnapshot } from "@/core/system-preferences";

function TestWorkspace({ projectId }: { projectId: string }) {
  return <TerminalWorkspace projectId={projectId} />;
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
const renderer = vi.hoisted(() => ({ create: vi.fn(), open: vi.fn(), fit: vi.fn(), write: vi.fn(), refresh: vi.fn(), clearTextureAtlas: vi.fn(), dispose: vi.fn(), input: vi.fn<(data: string) => void>() as (data: string) => void }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() { renderer.fit(); } } }));
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    options: ITerminalOptions;
    constructor(options: ITerminalOptions) { this.options = options; renderer.create(options); }
    cols = 100;
    rows = 24;
    loadAddon() {}
    open(element: HTMLElement) { renderer.open(element); }
    onData(callback: (data: string) => void) { renderer.input = callback; return { dispose() { renderer.input = () => {}; } }; }
    write(data: string) { renderer.write(data); }
    refresh(start: number, end: number) { renderer.refresh(start, end); }
    clearTextureAtlas() { renderer.clearTextureAtlas(); }
    reset() {}
    focus() {}
    dispose() { renderer.dispose(); }
  },
}));

const terminals = [
  { id: "terminal-1", projectId: "chat", conversationId: null, title: "Terminal 1", cwd: "/project", pid: 1, startedAt: 1, endedAt: null, exitCode: null, status: "running", origin: "user" },
  { id: "terminal-2", projectId: "chat", conversationId: null, title: "Terminal 2", cwd: "/project", pid: 2, startedAt: 2, endedAt: null, exitCode: null, status: "running", origin: "user" },
] as const;

const service: ProjectTerminal = { id: "service", origin: "agent", projectId: "chat", conversationId: "origin-chat", title: "Vite", command: "bun run dev", cwd: "/project", pid: 123, startedAt: 1, endedAt: null, exitCode: null, status: "running" };
const systemSnapshot: SystemSnapshot = {
  preferences: { preventSleep: "off", notifications: false, askUserTimeoutSeconds: 30, responseLanguage: "pt-BR", terminal: { shell: null, arguments: [], fontFamily: null, fontSize: 13 }, browser: { mode: "embedded", application: "chrome" } },
  sleepInhibited: false,
  sleepError: null,
  notificationError: null,
  availableTerminalShells: ["/bin/zsh", "/bin/bash"],
  availableTerminalFonts: ["NotoSansM Nerd Font Mono", "JetBrains Mono"],
  resolvedTerminalShell: "/bin/zsh",
  terminalError: null,
  terminalFontError: null,
};

describe("Integrated terminals", () => {
  it("shows an ended service as read-only and offers a fresh shell without replaying the service", async () => {
    const user = userEvent.setup();
    const ended = { ...service, status: "exited", endedAt: 10, exitCode: 130 };
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_project_terminals") return [ended];
      if (command === "read_project_terminal") return { terminal: ended, output: "^C", revision: 2, truncated: false };
      if (command === "create_project_terminal") return terminals[0];
      return undefined;
    });
    render(<TestWorkspace projectId="chat" />);
    expect(await screen.findByText("Processo encerrado · código 130")).toBeVisible();
    await waitFor(() => expect(renderer.create).toHaveBeenCalledWith(expect.objectContaining({ disableStdin: true })));
    await user.click(screen.getByRole("button", { name: "Abrir novo terminal" }));
    expect(invoke).toHaveBeenCalledWith("create_project_terminal", { projectId: "chat" });
    expect(invoke).not.toHaveBeenCalledWith("write_project_terminal", expect.anything());
  });
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
    vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_project_terminals") return [];
      if (command === "create_project_terminal") return terminals[created++];
      if (command === "get_system_preferences") return systemSnapshot;
      if (command === "read_project_terminal") {
        return { terminal: terminals[Math.min(created, 1)], output: "", revision: 0, truncated: false };
      }
      return undefined;
    });
  });

  it("restores project selection through navigation and restart without closing or replaying terminals", async () => {
    const legacy = { open: true, size: 57, activeTerminalId: "legacy-terminal" };
    let saved: DesktopLayout = { ...DEFAULT_DESKTOP_LAYOUT, terminalPanels: { "origin-chat": legacy } };
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_desktop_layout") return saved;
      if (command === "save_desktop_layout") saved = (args as { layout: DesktopLayout }).layout;
      if (command === "list_project_terminals") return (args as { projectId: string }).projectId === "chat" ? terminals : [];
      if (command === "read_project_terminal") return { terminal: terminals[1], output: "saved output", revision: 1, truncated: false };
    });
    const user = userEvent.setup();
    const workspace = (id: string) => <DesktopLayoutProvider><TestWorkspace key={id} projectId={id} /></DesktopLayoutProvider>;
    const view = render(workspace("chat"));
    await user.click(await screen.findByRole("tab", { name: "Terminal 2" }));
    await waitFor(() => expect(saved.terminalPanels["project:chat"].activeTerminalId).toBe("terminal-2"));
    view.rerender(workspace("other"));
    expect(await screen.findByText("Nenhum terminal aberto")).toBeVisible();
    view.rerender(workspace("chat"));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
    view.unmount();
    render(workspace("chat"));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
    expect(saved.terminalPanels["origin-chat"]).toEqual(legacy);
    expect(invoke).not.toHaveBeenCalledWith("create_project_terminal", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("write_project_terminal", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("close_project_terminal", expect.anything());
  });

  it("falls back to the first project terminal when the saved selection is stale", async () => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "get_desktop_layout") return { ...DEFAULT_DESKTOP_LAYOUT, terminalPanels: { "project:chat": { open: false, size: 40, activeTerminalId: "missing" } } };
      if (command === "list_project_terminals") return terminals;
      if (command === "read_project_terminal") return { terminal: terminals[0], output: "", revision: 0, truncated: false };
    });
    render(<DesktopLayoutProvider><TestWorkspace projectId="chat" /></DesktopLayoutProvider>);
    expect(await screen.findByRole("tab", { name: "Terminal 1" })).toHaveAttribute("aria-selected", "true");
  });

  it("keeps the newest project list when an older refresh completes late", async () => {
    let changed: EventCallback<unknown> = () => {};
    let resolveOld: (items: ProjectTerminal[]) => void = () => {};
    const old = new Promise<ProjectTerminal[]>(resolve => { resolveOld = resolve; });
    let lists = 0;
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (event === "terminals:changed") changed = callback;
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_project_terminals") return ++lists === 1 ? old : [terminals[0], service];
      if (command === "read_project_terminal") return { terminal: terminals[0], output: "", revision: 0, truncated: false };
    });
    render(<TestWorkspace projectId="chat" />);
    await waitFor(() => expect(lists).toBe(1));
    act(() => changed({ event: "terminals:changed", id: 1, payload: { projectId: "other" } }));
    expect(lists).toBe(1);
    act(() => changed({ event: "terminals:changed", id: 2, payload: { projectId: "chat" } }));
    expect(await screen.findByRole("tab", { name: "Vite" })).toBeVisible();
    await act(async () => resolveOld([terminals[1]]));
    expect(screen.getByRole("tab", { name: "Terminal 1" })).toBeVisible();
    expect(screen.queryByRole("tab", { name: "Terminal 2" })).not.toBeInTheDocument();
  });

  it("ignores a previous project's late list and creation after switching projects", async () => {
    let resolveList: (items: ProjectTerminal[]) => void = () => {};
    let resolveCreate: (item: ProjectTerminal) => void = () => {};
    const pendingList = new Promise<ProjectTerminal[]>(resolve => { resolveList = resolve; });
    const pendingCreate = new Promise<ProjectTerminal>(resolve => { resolveCreate = resolve; });
    let lists = 0;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_project_terminals") return ++lists === 2 ? pendingList : [];
      if (command === "create_project_terminal") return pendingCreate;
      if (command === "read_project_terminal") return { terminal: service, output: "", revision: 0, truncated: false };
      if (command === "get_system_preferences") return systemSnapshot;
      if (command === "save_desktop_layout") throw new Error(`Unexpected layout save: ${String(args)}`);
    });
    const user = userEvent.setup();
    const view = render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    view.rerender(<TestWorkspace projectId="other" />);
    await waitFor(() => expect(lists).toBe(2));
    view.rerender(<TestWorkspace projectId="third" />);
    expect(await screen.findByText("Nenhum terminal aberto")).toBeVisible();
    await act(async () => { resolveList([service]); resolveCreate(terminals[0]); });
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    expect(toast.success).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("close_project_terminal", expect.anything());
  });

  it("drops a previous project's pending close dialog and ignores its late completion", async () => {
    let resolveClose: () => void = () => {};
    const pendingClose = new Promise<void>(resolve => { resolveClose = resolve; });
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_project_terminals") return (args as { projectId: string }).projectId === "chat" ? [service] : [];
      if (command === "read_project_terminal") return { terminal: service, output: "", revision: 0, truncated: false };
      if (command === "close_project_terminal") return pendingClose;
    });
    const user = userEvent.setup();
    const view = render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("button", { name: "Fechar Vite" }));
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    view.rerender(<TestWorkspace projectId="other" />);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(await screen.findByText("Nenhum terminal aberto")).toBeVisible();
    await act(async () => resolveClose());
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    expect(toast.success).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("close_project_terminal", { projectId: "chat", id: "service", confirmed: true });
  });

  it("hydrates project output once and ignores other projects and disposed listeners", async () => {
    let output: EventCallback<unknown> = () => {};
    let resolveRead: (snapshot: unknown) => void = () => {};
    const snapshot = new Promise<unknown>(resolve => { resolveRead = resolve; });
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (event === "terminals:output") output = callback;
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_project_terminals") return [service];
      if (command === "read_project_terminal") return snapshot;
      if (command === "get_system_preferences") return systemSnapshot;
    });
    const view = render(<TestWorkspace projectId="chat" />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("read_project_terminal", { projectId: "chat", id: "service" }));
    const emit = (projectId: string, revision: number, data: string) => output({ event: "terminals:output", id: revision, payload: { projectId, id: "service", revision, data } });
    act(() => { emit("other", 4, "foreign"); emit("chat", 4, "new output"); });
    await act(async () => resolveRead({ terminal: service, output: "saved output", revision: 3, truncated: false }));
    expect(renderer.write.mock.calls.map(([data]) => data)).toEqual(["saved output", "new output"]);
    act(() => emit("chat", 3, "duplicate"));
    view.unmount();
    act(() => emit("chat", 5, "late output"));
    expect(renderer.write.mock.calls.map(([data]) => data)).toEqual(["saved output", "new output"]);
    expect(invoke).not.toHaveBeenCalledWith("write_project_terminal", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("close_project_terminal", expect.anything());
  });

  it("waits for the configured terminal font before measuring and fitting the terminal", async () => {
    const fontFamily = terminalFontFamily("NotoSansM Nerd Font Mono");
    let resolveFont: (fonts: FontFace[]) => void = () => {};
    const loadFont = vi.fn(() => new Promise<FontFace[]>(resolve => { resolveFont = resolve; }));
    Object.defineProperty(document, "fonts", { configurable: true, value: { load: loadFont } });
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    const surface = await screen.findByRole("application", { name: "Terminal Terminal 1" });
    Object.defineProperties(surface, { clientWidth: { value: 1_080 }, clientHeight: { value: 580 } });

    expect(loadFont).toHaveBeenCalledWith(`13px ${fontFamily}`);
    expect(renderer.open).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("resize_project_terminal", expect.anything());
    await act(async () => resolveFont([]));

    await waitFor(() => expect(renderer.open).toHaveBeenCalledWith(surface));
    expect(renderer.create).toHaveBeenCalledWith(expect.objectContaining({ fontFamily, fontSize: 13, letterSpacing: 0 }));
    expect(renderer.fit).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("resize_project_terminal", { projectId: "chat", id: "terminal-1", rows: 24, cols: 100 });
    expect(renderer.write).toHaveBeenCalled();
  });

  it("opens the terminal even when the bundled font cannot be loaded", async () => {
    Object.defineProperty(document, "fonts", { configurable: true, value: { load: vi.fn().mockRejectedValue(new Error("Font unavailable")) } });
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    await waitFor(() => expect(renderer.open).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("read_project_terminal", { projectId: "chat", id: "terminal-1" });
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("applies font changes to an open terminal and refits it", async () => {
    let changed: EventCallback<unknown> = () => {};
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (event === "system:changed") changed = callback;
      return () => {};
    });
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    await waitFor(() => expect(renderer.open).toHaveBeenCalled());
    const surface = screen.getByRole("application", { name: "Terminal Terminal 1" });
    Object.defineProperties(surface, { clientWidth: { value: 1_080 }, clientHeight: { value: 580 } });
    const before = renderer.fit.mock.calls.length;

    act(() => changed({
      event: "system:changed",
      id: 1,
      payload: {
        ...systemSnapshot,
        availableTerminalFonts: ["MesloLGS NF", ...systemSnapshot.availableTerminalFonts],
        preferences: { ...systemSnapshot.preferences, terminal: { ...systemSnapshot.preferences.terminal, fontFamily: "MesloLGS NF", fontSize: 16 } },
      },
    }));

    await waitFor(() => expect(renderer.create.mock.calls[renderer.create.mock.calls.length - 1]?.[0]).toEqual(expect.objectContaining({ fontFamily: '"MesloLGS NF", "JetBrains Mono", monospace', fontSize: 16 })));
    expect(renderer.clearTextureAtlas).toHaveBeenCalledOnce();
    expect(renderer.refresh).toHaveBeenCalledWith(0, 23);
    expect(renderer.fit.mock.calls.length).toBeGreaterThan(before);
  });

  it("does not open or resize a terminal after closing while its font loads", async () => {
    let resolveFont: (fonts: FontFace[]) => void = () => {};
    Object.defineProperty(document, "fonts", { configurable: true, value: { load: () => new Promise<FontFace[]>(resolve => { resolveFont = resolve; }) } });
    const user = userEvent.setup();
    const view = render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("button", { name: "Novo Terminal" }));
    await screen.findByRole("application");
    view.unmount();
    await act(async () => resolveFont([]));

    expect(renderer.dispose).toHaveBeenCalled();
    expect(renderer.open).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("read_project_terminal", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("resize_project_terminal", expect.anything());
  });

  it.each([String.raw`C:\Users\pauli\Documents\projetos\ação [teste]`, String.raw`\\servidor\projetos\Jarvis`, "/Users/pauli/projects/Jarvis"])("shows the full project path in the footer tooltip: %s", async cwd => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_project_terminals") return [{ ...terminals[0], cwd }];
      if (command === "read_project_terminal") return { terminal: { ...terminals[0], cwd }, output: "", revision: 0, truncated: false };
      return undefined;
    });
    render(<TestWorkspace projectId="chat" />);
    expect(await screen.findByText(cwd)).toHaveTextContent(cwd);
    expect(screen.getByText("Em execução")).toBeVisible();
  });

  it("creates tabs, renames them, and requires confirmation before closing", async () => {
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    expect(await screen.findByText("Nenhum terminal aberto")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Novo Terminal" }));
    await screen.findByRole("tab", { name: "Terminal 1" });
    await user.click(screen.getByRole("button", { name: "Novo terminal" }));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("create_project_terminal", { projectId: "chat" });

    const firstTab = await screen.findByRole("tab", { name: "Terminal 1" });
    fireEvent.contextMenu(firstTab);
    await user.click(await screen.findByRole("menuitem", { name: "Renomear" }));
    const name = await screen.findByRole("textbox", { name: "Novo nome para Terminal 1" });
    await user.clear(name);
    await user.type(name, "Console");
    await user.click(screen.getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("rename_project_terminal", { projectId: "chat", id: "terminal-1", title: "Console" }));

    await user.click(screen.getByRole("button", { name: "Fechar Console" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Fechar Console?");
    expect(invoke).not.toHaveBeenCalledWith("close_project_terminal", expect.anything());
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("close_project_terminal", { projectId: "chat", id: "terminal-1", confirmed: true }));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
  });

  it("shows one terminal tab strip and creates terminals after the last tab", async () => {
    const third = { ...terminals[0], id: "terminal-3", title: "Terminal 3" };
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_project_terminals") return terminals;
      if (command === "create_project_terminal") return third;
      if (command === "read_project_terminal") return { terminal: (args as { id: string }).id === third.id ? third : terminals[0], output: "", revision: 0, truncated: false };
    });
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    expect(screen.queryByRole("tab", { name: "Processos" })).not.toBeInTheDocument();
    await screen.findByRole("tab", { name: "Terminal 1" });
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
      if (command === "list_project_terminals") return terminals;
      if (command === "read_project_terminal") return { terminal: terminals.find(terminal => terminal.id === (args as { id: string }).id), output: "", revision: 0, truncated: false };
    });
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    const selected = await screen.findByRole("tab", { name: "Terminal 1" });
    const target = terminals[index];
    const close = screen.getByRole("button", { name: `Fechar ${target.title}` });
    expect(close).toHaveAttribute("aria-haspopup", "dialog");

    await user.click(close);
    expect(screen.getByRole("alertdialog")).toHaveTextContent(`Fechar ${target.title}?`);
    expect(selected).toHaveAttribute("aria-selected", "true");
    expect(invoke).not.toHaveBeenCalledWith("close_project_terminal", expect.anything());
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(selected).toHaveAttribute("aria-selected", "true");

    await user.click(close);
    expect(screen.getByRole("alertdialog")).toHaveTextContent(`Fechar ${target.title}?`);
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(invoke).toHaveBeenCalledWith("close_project_terminal", { projectId: "chat", id: target.id, confirmed: true });
    expect(screen.queryByRole("tab", { name: target.title })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: terminals[1 - index].title })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("region", { name: "Terminais do projeto" })).toBeVisible();
  });

  it("keeps the requested terminal and confirmation available after a close failure", async () => {
    let fail = true;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_project_terminals") return terminals;
      if (command === "read_project_terminal") return { terminal: terminals.find(terminal => terminal.id === (args as { id: string }).id), output: "", revision: 0, truncated: false };
      if (command === "close_project_terminal" && fail) throw { message: "Falha ao fechar terminal." };
    });
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("button", { name: "Fechar Terminal 2" }));
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
      if (command === "list_project_terminals") return [...terminals, item];
      if (command === "read_project_terminal") return { terminal: (args as {id: string}).id === item.id ? item : terminals[0], output: "Local: http://localhost:1420/", revision: 1, truncated: false };
    });
    const user = userEvent.setup();
    render(<TestWorkspace projectId="chat" />);
    await user.click(await screen.findByRole("tab", { name: "Vite" }));
    await waitFor(() => expect(renderer.write).toHaveBeenCalledWith("Local: http://localhost:1420/"));
    expect(screen.getByRole("application", { name: "Terminal Vite" })).toBeVisible();
    expect(screen.queryByRole("tab", { name: "Processos" })).not.toBeInTheDocument();
    if (status === "running") {
      act(() => renderer.input("r\r"));
      await waitFor(() => expect(invoke).toHaveBeenCalledWith("write_project_terminal", { projectId: "chat", id: "service", input: "r\r" }));
    }
    await user.click(screen.getByRole("button", { name: "Fechar Vite" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Fechar Vite?");
    expect(invoke).not.toHaveBeenCalledWith("close_project_terminal", expect.anything());
    await user.click(screen.getByRole("button", { name: "Fechar terminal" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("close_project_terminal", { projectId: "chat", id: "service", confirmed: true }));
    expect(screen.queryByRole("tab", { name: "Vite" })).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("list_chat_processes", expect.anything());
  });

  it("does not leak service terminals across projects", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_project_terminals") return (args as { projectId: string }).projectId === "chat" ? [service] : [];
      if (command === "read_project_terminal") return { terminal: service, output: "", revision: 0, truncated: false };
    });
    const view = render(<TestWorkspace projectId="chat" />);
    expect(await screen.findByRole("tab", { name: "Vite" })).toBeVisible();
    view.rerender(<TestWorkspace projectId="other" />);
    expect(screen.queryByRole("tab", { name: "Vite" })).not.toBeInTheDocument();
    expect(await screen.findByText("Nenhum terminal aberto")).toBeVisible();
    expect(invoke).not.toHaveBeenCalledWith("close_project_terminal", expect.anything());
  });
});
