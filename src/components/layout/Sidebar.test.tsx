import { StrictMode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useLibrary } from "@/hooks/use-library";
import { emptyLibrary, populatedLibrary } from "@/test/library-fixtures";
import { AppSidebar } from "./Sidebar";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
function Harness({ runningIds, unreadIds }: { runningIds?: ReadonlySet<string>; unreadIds?: ReadonlySet<string> }) {
  return <AppSidebar library={useLibrary()} runningConversationIds={runningIds} unreadConversationIds={unreadIds} />;
}

describe("Persistent sidebar", () => {
  it("keeps the project group current on chat and dashboard, then moves it to the selected project", async () => {
    const user = userEvent.setup();
    const stored = populatedLibrary();
    stored.projects.push({ id: "p3", workspaceId: "w1", name: "Website", path: "/projects/website", createdAt: 3 });
    call.mockResolvedValue(stored);
    render(<Harness />);
    const jarvis = await screen.findByRole("group", { name: "Projeto Jarvis" });
    const website = screen.getByRole("group", { name: "Projeto Website" });
    expect(jarvis).toHaveAttribute("aria-current", "true");
    expect(website).not.toHaveAttribute("aria-current");
    expect(within(jarvis).getByRole("button", { name: "Primeira conversa" })).toHaveAttribute("aria-current", "page");
    call.mockResolvedValue({ ...stored, selection: { ...stored.selection, conversationId: null } });
    await user.click(within(jarvis).getByRole("button", { name: "Dashboard" }));
    await waitFor(() => expect(within(jarvis).getByRole("button", { name: "Dashboard" })).toHaveAttribute("aria-current", "page"));
    expect(jarvis).toHaveAttribute("aria-current", "true");
    call.mockResolvedValue({ ...stored, selection: { ...stored.selection, projectId: "p3", conversationId: null } });
    await user.click(within(website).getByRole("button", { name: "Website" }));
    await waitFor(() => expect(website).toHaveAttribute("aria-current", "true"));
    expect(jarvis).not.toHaveAttribute("aria-current");
  });

  it("marks unread conversations and folded projects without hiding the running indicator", async () => {
    const stored = populatedLibrary();
    stored.projects.push({ id:"p3", workspaceId:"w1", name:"Website", path:"/projects/website", createdAt:3 });
    stored.conversations.push({ id:"c3", projectId:"p3", title:"Background chat", createdAt:3 });
    call.mockResolvedValue(stored);
    const view = render(<Harness runningIds={new Set(["c1"])} unreadIds={new Set(["c1", "c3"])} />);
    const selected = await screen.findByRole("button", { name:/Primeira conversa/ });
    expect(within(selected).getByRole("img", { name:"Mensagem não lida" })).toBeVisible();
    expect(within(selected).getByRole("status", { name:"Conversa em execução" })).toBeVisible();
    expect(within(screen.getByTitle("/projects/website")).getByRole("img", { name:"Projeto com mensagens não lidas" })).toBeVisible();
    view.rerender(<Harness unreadIds={new Set(["c3"])} />);
    expect(within(selected).queryByRole("img", { name:"Mensagem não lida" })).not.toBeInTheDocument();
    expect(within(screen.getByTitle("/projects/website")).getByRole("img", { name:"Projeto com mensagens não lidas" })).toBeVisible();
  });
  it("opens the Dashboard first and reveals recent sessions three then ten at a time", async () => {
    const user = userEvent.setup();
    const stored = populatedLibrary();
    stored.conversations = Array.from({ length: 25 }, (_, index) => ({ id: `chat${index}`, projectId: "p1", title: `Sessão ${index}`, createdAt: index, lastActivityAt: index === 0 ? 100 : index }));
    stored.selection.conversationId = "chat0";
    call.mockResolvedValue(stored);
    render(<Harness />);
    const dashboard = await screen.findByRole("button", { name: "Dashboard" });
    const menu = screen.getByRole("list", { name: "Conversas do projeto" });
    expect(within(menu).getAllByRole("button").map(button => button.textContent)).toEqual(["Sessão 0", "Sessão 24", "Sessão 23"]);
    await user.click(screen.getByRole("button", { name: /Ver mais/ }));
    expect(within(menu).getAllByRole("button")).toHaveLength(13);
    await user.click(screen.getByRole("button", { name: /Ver mais/ }));
    expect(within(menu).getAllByRole("button")).toHaveLength(23);
    await user.click(screen.getByRole("button", { name: /Ver mais/ }));
    expect(within(menu).getAllByRole("button")).toHaveLength(25);
    expect(screen.queryByRole("button", { name: /Ver mais/ })).not.toBeInTheDocument();
    call.mockResolvedValue({ ...stored, selection: { ...stored.selection, conversationId: null } });
    await user.click(dashboard);
    expect(call).toHaveBeenLastCalledWith("select_library_item", { target: { kind: "project", id: "p1" } });
    await waitFor(() => expect(dashboard).toHaveAttribute("aria-current", "page"));
  });
  beforeEach(() => {
    call.mockReset();
    call.mockResolvedValue(emptyLibrary());
  });

  it("shows execution on conversations and folded projects, and clears it when finished", async () => {
    const stored = populatedLibrary();
    stored.projects.push({ id: "p3", workspaceId: "w1", name: "Website", path: "/projects/website", createdAt: 3 });
    stored.conversations.push({ id: "c3", projectId: "p3", title: "Conversa em segundo plano", createdAt: 3 });
    call.mockResolvedValue(stored);
    const { rerender } = render(<Harness runningIds={new Set(["c1", "c3"])} />);
    const selected = await screen.findByRole("button", { name: /Primeira conversa/ });
    expect(within(selected).getByRole("status", { name: "Conversa em execução" })).toBeInTheDocument();
    const folded = screen.getByTitle("/projects/website");
    expect(within(folded).getByRole("status", { name: "Projeto com conversa em execução" })).toBeInTheDocument();
    expect(screen.queryByText("Conversa em segundo plano")).not.toBeInTheDocument();
    rerender(<Harness runningIds={new Set()} />);
    expect(screen.queryByRole("status", { name: /em execução/ })).not.toBeInTheDocument();
    expect(selected).toHaveAttribute("aria-current", "page");
  });

  it("confirms permanent conversation deletion from the tree and supports cancellation", async () => {
    const user = userEvent.setup();
    const initial = populatedLibrary();
    call.mockResolvedValueOnce(initial);
    render(<Harness />);
    await screen.findByText("Primeira conversa");
    const panel = screen.getByRole("complementary", { name: "Workspace" });
    fireEvent.contextMenu(within(panel).getByRole("button", { name: "Primeira conversa" }));
    await user.click(await screen.findByRole("menuitem", { name: "Excluir" }));
    let dialog = screen.getByRole("alertdialog", { name: "Excluir conversa?" });
    expect(dialog).toHaveTextContent("Primeira conversa");
    expect(dialog).toHaveTextContent("exclusão é definitiva");
    expect(dialog).toHaveTextContent("arquivos do projeto permanecerão intactos");
    expect(call).not.toHaveBeenCalledWith("delete_library_item", expect.anything());
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Excluir conversa" })).toHaveFocus());
    await user.tab({ shift: true });
    expect(within(dialog).getByRole("button", { name: "Cancelar" })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Primeira conversa" })).toBeInTheDocument();
    fireEvent.contextMenu(within(panel).getByRole("button", { name: "Primeira conversa" }));
    await user.click(await screen.findByRole("menuitem", { name: "Excluir" }));
    dialog = screen.getByRole("alertdialog");
    const result = structuredClone(initial);
    result.conversations = result.conversations.filter(item => item.id !== "c1");
    result.selection.conversationId = null;
    call.mockResolvedValueOnce(result);
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Excluir conversa" })).toHaveFocus());
    await user.keyboard("{Enter}");
    expect(call).toHaveBeenLastCalledWith("delete_library_item", { target: { kind: "conversation", id: "c1" }, confirmed: true });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "Primeira conversa" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Nova conversa em Jarvis" })).toBeEnabled();
  });

  it("confirms deletion of the right-clicked project, preserves other selection and allows retry", async () => {
    const user = userEvent.setup();
    const initial = populatedLibrary();
    initial.projects.push({ id: "p3", workspaceId: "w1", name: "Website", path: "/projects/website", createdAt: 3 });
    initial.conversations.push({ id: "c3", projectId: "p3", title: "Site", createdAt: 3 });
    call.mockResolvedValueOnce(initial);
    render(<Harness />);
    fireEvent.contextMenu(await screen.findByRole("button", { name: "Website" }));
    await user.click(await screen.findByRole("menuitem", { name: "Excluir" }));
    const dialog = screen.getByRole("alertdialog", { name: "Excluir projeto?" });
    expect(dialog).toHaveTextContent("Website");
    expect(dialog).toHaveTextContent("todas as suas conversas (1)");
    expect(dialog).toHaveTextContent("A pasta do projeto e seus arquivos permanecerão intactos");
    call.mockRejectedValueOnce({ message: "Interrompa as conversas em execução antes de excluir este item." });
    await user.click(within(dialog).getByRole("button", { name: "Excluir projeto" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Interrompa as conversas");
    let finish!: (value: unknown) => void;
    call.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    await user.dblClick(within(dialog).getByRole("button", { name: "Excluir projeto" }));
    expect(within(dialog).getByRole("button", { name: "Cancelar" })).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(dialog).toBeInTheDocument();
    expect(call.mock.calls.filter(([name]) => name === "delete_library_item")).toHaveLength(2);
    const result = structuredClone(initial);
    result.projects = result.projects.filter(item => item.id !== "p3");
    result.conversations = result.conversations.filter(item => item.projectId !== "p3");
    await act(async () => finish(result));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "Website" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Nova conversa em Website" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Primeira conversa" })).toHaveAttribute("aria-current", "page");
    expect(call).not.toHaveBeenCalledWith("select_library_item", expect.anything());
    expect(call).toHaveBeenLastCalledWith("delete_library_item", { target: { kind: "project", id: "p3" }, confirmed: true });
  });

  it("loads an empty library in StrictMode and creates a named workspace", async () => {
    const user = userEvent.setup();
    render(
      <StrictMode>
        <Harness />
      </StrictMode>,
    );
    expect(
      await screen.findByText("Organize seus projetos"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Jarvis")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Novo" }));
    expect(await screen.findByRole("menuitem", { name: "Novo projeto" })).toHaveAttribute("aria-disabled", "true");
    expect(screen.queryByRole("menuitem", { name: "Nova conversa" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("menuitem", { name: "Novo workspace" }));
    const dialog = screen.getByRole("dialog", { name: "Novo workspace" });
    await user.type(
      within(dialog).getByLabelText("Nome do workspace"),
      "  Pessoal  ",
    );
    const created = emptyLibrary();
    created.workspaces = populatedLibrary().workspaces.slice(0, 1);
    created.selection.workspaceId = "w1";
    call.mockResolvedValueOnce(created);
    await user.click(
      within(dialog).getByRole("button", { name: "Criar workspace" }),
    );
    expect(call).toHaveBeenCalledWith("create_workspace", { name: "Pessoal" });
    expect(await screen.findByText("Nenhum projeto")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("combobox", { name: "Selecionar workspace" }),
    ).toHaveTextContent("Pessoal");
  });

  it("uses the native picker, preserves selection on cancellation, and displays the returned project", async () => {
    const user = userEvent.setup();
    const initial = emptyLibrary();
    initial.workspaces = populatedLibrary().workspaces.slice(0, 1);
    initial.selection.workspaceId = "w1";
    call.mockResolvedValueOnce(initial);
    render(<Harness />);
    await screen.findByText("Nenhum projeto");
    await user.click(screen.getByRole("button", { name: "Novo" }));
    const add = await screen.findByRole("menuitem", { name: "Novo projeto" });
    call.mockResolvedValueOnce(null);
    await user.click(add);
    expect(call).toHaveBeenCalledWith("add_project", { workspaceId: "w1" });
    expect(screen.getByText("Nenhum projeto")).toBeInTheDocument();
    call.mockResolvedValueOnce({
      ...populatedLibrary(),
      conversations: [],
      selection: { workspaceId: "w1", projectId: "p1", conversationId: null },
    });
    await user.click(screen.getByRole("button", { name: "Novo" }));
    await user.click(await screen.findByRole("menuitem", { name: "Novo projeto" }));
    expect(await screen.findByRole("button", { name: "Jarvis" })).toHaveAttribute("title", "/projects/jarvis");
    expect(screen.getByRole("button", { name: "Nova conversa em Jarvis" })).toBeEnabled();
  });

  it("creates a conversation immediately without a title prompt and prevents duplicate submissions", async () => {
    const user = userEvent.setup();
    call.mockResolvedValueOnce(populatedLibrary());
    render(<Harness />);
    await screen.findByText("Primeira conversa");
    let finish!: (value: unknown) => void;
    call.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const create = screen.getByRole("button", { name: "Nova conversa em Jarvis" });
    await user.dblClick(create);
    expect(
      call.mock.calls.filter(([command]) => command === "create_conversation"),
    ).toHaveLength(1);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(create).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Novo" }),
    ).toBeDisabled();
    await act(async () => finish({ bad: "payload" }));
    expect(screen.getByRole("alert")).toHaveTextContent("dados recebidos");
    const created = populatedLibrary();
    created.conversations.unshift({
      id: "c3",
      projectId: "p1",
      title: "Nova Conversa",
      createdAt: 3,
    });
    created.selection.conversationId = "c3";
    call.mockResolvedValueOnce(created);
    expect(create).toBeEnabled();
    await user.click(create);
    expect(call).toHaveBeenLastCalledWith("create_conversation", {
      projectId: "p1",
    });
    expect(
      await screen.findByRole("button", { name: "Nova Conversa" }),
    ).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Jarvis" })).toHaveAttribute("aria-expanded", "true");
  });

  it.each(["pointer", "keyboard"])("creates a conversation in the collapsed, unselected project using the %s", async (interaction) => {
    const user = userEvent.setup();
    const stored = populatedLibrary();
    stored.projects.push({ id: "p3", workspaceId: "w1", name: "Website", path: "/projects/website", createdAt: 3 });
    stored.conversations.push({ id: "c3", projectId: "p3", title: "Conversa anterior", createdAt: 3 });
    call.mockResolvedValueOnce(stored);
    render(<Harness />);
    const project = await screen.findByRole("button", { name: "Website" });
    expect(project).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: "Conversa anterior" })).not.toBeInTheDocument();
    const created = structuredClone(stored);
    created.conversations.push({ id: "c4", projectId: "p3", title: "Nova Conversa", createdAt: 4 });
    created.selection = { workspaceId: "w1", projectId: "p3", conversationId: "c4" };
    call.mockResolvedValueOnce(created);
    const create = screen.getByRole("button", { name: "Nova conversa em Website" });
    if (interaction === "keyboard") {
      project.focus();
      await user.tab();
      expect(create).toHaveFocus();
      await user.keyboard("{Enter}");
    } else {
      await user.click(create);
    }
    expect(call).toHaveBeenLastCalledWith("create_conversation", { projectId: "p3" });
    expect(call).not.toHaveBeenCalledWith("select_library_item", expect.anything());
    expect(await screen.findByRole("button", { name: "Nova Conversa" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Website" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "Conversa anterior" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("edits the right-clicked project without changing its folder or selecting it", async () => {
    const user = userEvent.setup();
    const stored = populatedLibrary();
    stored.projects.push({
      id: "p3",
      workspaceId: "w1",
      name: "Website",
      path: "/projects/website",
      createdAt: 3,
    });
    call.mockResolvedValueOnce(stored);
    render(<Harness />);
    const target = await screen.findByRole("button", {
      name: "Website",
    });
    fireEvent.contextMenu(target);
    await user.click(await screen.findByRole("menuitem", { name: "Editar" }));
    const dialog = screen.getByRole("dialog", { name: "Editar projeto" });
    const name = within(dialog).getByLabelText("Nome do projeto");
    expect(name).toHaveValue("Website");
    expect(within(dialog).getByLabelText("Local do projeto")).toHaveValue(
      "/projects/website",
    );
    expect(within(dialog).getByLabelText("Local do projeto")).toHaveAttribute(
      "readonly",
    );
    await user.clear(name);
    expect(
      within(dialog).getByRole("button", { name: "Salvar" }),
    ).toBeDisabled();
    await user.type(name, "  Meu site  ");
    const renamed = structuredClone(stored);
    renamed.projects[2].name = "Meu site";
    call.mockResolvedValueOnce(renamed);
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    expect(call).toHaveBeenLastCalledWith("rename_project", {
      id: "p3",
      name: "Meu site",
    });
    expect(
      await screen.findByRole("button", {
        name: "Meu site",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Primeira conversa" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      call.mock.calls.filter(([command]) => command === "select_library_item"),
    ).toHaveLength(0);
  });

  it(
    "edits a conversation from the tree and permits retry after failure",
    async () => {
      const user = userEvent.setup();
      call.mockResolvedValueOnce(populatedLibrary());
      render(<Harness />);
      await screen.findByText("Primeira conversa");
      const panel = screen.getByRole("complementary", { name: "Workspace" });
      fireEvent.contextMenu(
        within(panel).getByRole("button", { name: "Primeira conversa" }),
      );
      await user.click(await screen.findByRole("menuitem", { name: "Editar" }));
      const input = screen.getByLabelText("Título da conversa");
      expect(input).toHaveValue("Primeira conversa");
      await user.clear(input);
      await user.type(input, "Revisão da autenticação");
      call.mockRejectedValueOnce({ message: "Não foi possível salvar." });
      await user.click(screen.getByRole("button", { name: "Salvar" }));
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Não foi possível salvar.",
      );
      expect(input).toHaveValue("Revisão da autenticação");
      const renamed = populatedLibrary();
      renamed.conversations[0].title = "Revisão da autenticação";
      call.mockResolvedValueOnce(renamed);
      await user.click(screen.getByRole("button", { name: "Salvar" }));
      expect(call).toHaveBeenLastCalledWith("rename_conversation", {
        id: "c1",
        title: "Revisão da autenticação",
      });
      expect(
        await within(panel).findByRole("button", {
          name: "Revisão da autenticação",
        }),
      ).toHaveAttribute("aria-current", "page");
    },
  );

  it("scopes projects to the selected workspace and restores selection from the backend", async () => {
    const user = userEvent.setup();
    call.mockResolvedValueOnce(populatedLibrary());
    render(<Harness />);
    expect(await screen.findByText("Primeira conversa")).toBeInTheDocument();
    expect(screen.queryByText("Outro projeto")).not.toBeInTheDocument();
    await user.click(
      screen.getByRole("combobox", { name: "Selecionar workspace" }),
    );
    const next = populatedLibrary();
    next.selection = {
      workspaceId: "w2",
      projectId: null,
      conversationId: null,
    };
    call.mockResolvedValueOnce(next);
    await user.click(await screen.findByRole("option", { name: "Trabalho" }));
    expect(call).toHaveBeenLastCalledWith("select_library_item", {
      target: { kind: "workspace", id: "w2" },
    });
    expect(await screen.findByText("Outro projeto")).toBeInTheDocument();
    expect(screen.queryByText("Primeira conversa")).not.toBeInTheDocument();
    next.selection = {
      workspaceId: "w2",
      projectId: "p2",
      conversationId: null,
    };
    call.mockResolvedValueOnce(next);
    await user.click(screen.getByRole("button", { name: "Outro projeto" }));
    expect(
      await screen.findByRole("button", { name: "Conversa do trabalho" }),
    ).toBeInTheDocument();
    next.selection.conversationId = "c2";
    call.mockResolvedValueOnce(next);
    await user.click(
      screen.getByRole("button", { name: "Conversa do trabalho" }),
    );
    expect(call).toHaveBeenLastCalledWith("select_library_item", {
      target: { kind: "conversation", id: "c2" },
    });
    expect(screen.getByRole("button", { name: "Conversa do trabalho" })).toHaveAttribute("aria-current", "page");
  });

  it("limits the global menu to workspace and project creation and preserves selection when collapsing projects", async () => {
    const user = userEvent.setup();
    call.mockResolvedValue(populatedLibrary());
    render(<Harness />);
    await screen.findByText("Primeira conversa");
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Novo" })).toHaveLength(1);
    const project = screen.getByRole("button", { name: "Jarvis" });
    expect(project).toHaveTextContent(/^Jarvis$/);
    await user.click(project);
    await waitFor(() => expect(screen.queryByRole("button", { name: "Primeira conversa" })).not.toBeInTheDocument());
    expect(call).not.toHaveBeenCalledWith("select_library_item", expect.anything());
    await user.click(project);
    expect(await screen.findByRole("button", { name: "Primeira conversa" })).toHaveAttribute("aria-current", "page");
    await user.click(screen.getByRole("button", { name: "Novo" }));
    expect((await screen.findAllByRole("menuitem")).map(item => item.textContent)).toEqual(["Novo projeto", "Novo workspace"]);
  });

  it("offers retry after loading fails and keeps Settings available", async () => {
    const user = userEvent.setup();
    call.mockRejectedValueOnce({ message: "Falha ao ler os projetos." });
    render(<Harness />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Falha ao ler os projetos.",
    );
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(
      await screen.findByText("Organize seus projetos"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Configurações" })).not.toBeInTheDocument();
  });
});
