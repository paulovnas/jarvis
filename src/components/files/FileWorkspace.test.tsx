import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { DesktopLayoutProvider } from "@/components/layout/DesktopLayoutProvider";
import { DEFAULT_DESKTOP_LAYOUT } from "@/core/desktop-layout";
import { DesktopLayoutContext } from "@/core/desktop-layout";
import type { LayoutUpdate } from "@/core/desktop-layout";
import type { BrowserController } from "@/hooks/use-browser";
import type { FilePreview } from "@/core/project-files";
import { useProjectFiles } from "@/hooks/use-project-files";
import { FileWorkspace } from "./FileWorkspace";
import { ProjectExplorer } from "./ProjectExplorer";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./CodeViewer", () => ({ default: ({ file }: { file: FilePreview }) => <div role="textbox" aria-label={`Arquivo ${file.path}`} aria-readonly="true">{file.content}</div> }));
const mockedInvoke = vi.mocked(invoke);

function ChatDraft() {
  const [value, setValue] = useState("");
  return <input aria-label="Rascunho do chat" value={value} onChange={event => setValue(event.target.value)} />;
}
function Workspace({ projectId = "project-1" }: { projectId?: string }) {
  const files = useProjectFiles(projectId);
  return <><ProjectExplorer key={projectId} projectId={projectId} projectName="Meu projeto" selected={files.tabs.activePath} onOpen={files.open} /><FileWorkspace files={files}><ChatDraft /></FileWorkspace></>;
}

it("restores mixed file and browser tab order while keeping Chat first and fixed", async () => {
  const user = userEvent.setup();
  const browser: BrowserController = { conversationId: "chat", snapshot: { activeId: null, tabs: [{ id: "web", conversationId: "chat", title: "Local", url: "http://localhost:5173", loading: false }] }, loaded: true, busy: false, command: vi.fn(), select: vi.fn(), open: vi.fn() };
  const layout = { ...DEFAULT_DESKTOP_LAYOUT, itemOrder: { "tabs:chat": ["browser:web", "file:README.md"] }, fileTabs: { "project-1": { paths: ["README.md"], activePath: null } } };
  function Mixed() { const files = useProjectFiles("project-1"); return <FileWorkspace files={files} browser={browser}><ChatDraft /></FileWorkspace>; }
  const update = vi.fn<(value: LayoutUpdate) => void>();
  render(<DesktopLayoutContext value={{ layout, updateLayout: update }}><Mixed /></DesktopLayoutContext>);
  await waitFor(() => expect(screen.getAllByRole("tab").map(tab => tab.textContent)).toEqual(["Chat", "Local", "README.md"]));
  expect(screen.getByRole("tab", { name: "Chat" })).not.toHaveAttribute("aria-describedby");
  const geometry = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const item = this.closest("[data-workspace-tab]");
    const index = item ? [...document.querySelectorAll("[data-workspace-tab]")].indexOf(item) : 0;
    return { x: index * 180, y: 0, top: 0, left: index * 180, right: (index + 1) * 180, bottom: 28, width: 180, height: 28, toJSON: () => ({}) };
  });
  try {
    screen.getByRole("tab", { name: "Local" }).focus();
    vi.mocked(browser.select).mockClear(); update.mockClear();
    await user.keyboard("[Space][ArrowRight][Space]");
    expect(update.mock.calls.map(([value]) => typeof value === "function" ? value(layout) : value)).toContainEqual({ itemOrder: { "tabs:chat": ["file:README.md", "browser:web"] } });
    expect(browser.select).not.toHaveBeenCalled();
  } finally { geometry.mockRestore(); }
});

beforeEach(() => {
  mockedInvoke.mockReset().mockImplementation(async (command, args) => {
    const input = args as { projectId: string; path: string } | undefined;
    if (command === "get_desktop_layout") return DEFAULT_DESKTOP_LAYOUT;
    if (command === "save_desktop_layout") return;
    if (command === "list_project_directory") return { path: input?.path, truncated: false, entries: input?.path === "src" ? [{ name: "app.ts", path: "src/app.ts", kind: "file" }] : [{ name: "src", path: "src", kind: "directory" }, { name: "README.md", path: "README.md", kind: "file" }] };
    if (command === "read_project_file") return { path: input?.path, content: `${input?.projectId}: ${input?.path}`, size: 20, encoding: "UTF-8" };
    throw new Error(`Unexpected ${command}`);
  });
});

it("opens read-only file tabs without closing or remounting Chat, deduplicates files and closes the requested tab", async () => {
  const user = userEvent.setup();
  render(<Workspace />);
  const draft = screen.getByRole("textbox", { name: "Rascunho do chat" });
  await user.type(draft, "Continuar de onde parei");
  await user.click(await screen.findByRole("treeitem", { name: "src" }));
  await user.click(await screen.findByRole("treeitem", { name: "app.ts" }));
  expect(await screen.findByRole("textbox", { name: "Arquivo src/app.ts" })).toHaveTextContent("project-1: src/app.ts");
  expect(screen.getByText("Somente leitura")).toBeVisible();
  expect(screen.queryByRole("button", { name: /Fechar.*Chat/ })).not.toBeInTheDocument();
  expect(draft).not.toBeVisible();
  await user.click(screen.getByRole("treeitem", { name: "app.ts" }));
  expect(screen.getAllByRole("tab", { name: "app.ts" })).toHaveLength(1);
  await user.click(screen.getByRole("treeitem", { name: "README.md" }));
  await screen.findByRole("textbox", { name: "Arquivo README.md" });
  await user.click(screen.getByRole("button", { name: "Fechar arquivo src/app.ts" }));
  expect(screen.getByRole("tab", { name: "README.md" })).toHaveAttribute("aria-selected", "true");
  await user.click(screen.getByRole("tab", { name: "Chat" }));
  expect(screen.getByRole("textbox", { name: "Rascunho do chat" })).toBe(draft);
  expect(draft).toHaveValue("Continuar de onde parei");
  await user.click(screen.getByRole("button", { name: "Fechar arquivo README.md" }));
  expect(screen.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
});

it("persists file tabs by project and restores them after remounting", async () => {
  const user = userEvent.setup();
  const first = render(<DesktopLayoutProvider><Workspace /></DesktopLayoutProvider>);
  await user.click(await screen.findByRole("treeitem", { name: "README.md" }));
  await screen.findByRole("textbox", { name: "Arquivo README.md" });
  await waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_desktop_layout", { layout: { ...DEFAULT_DESKTOP_LAYOUT, fileTabs: { "project-1": { paths: ["README.md"], activePath: "README.md" } } } }));
  first.unmount();
  mockedInvoke.mockImplementationOnce(async () => ({ ...DEFAULT_DESKTOP_LAYOUT, fileTabs: { "project-1": { paths: ["README.md"], activePath: "README.md" } } }));
  render(<DesktopLayoutProvider><Workspace /></DesktopLayoutProvider>);
  expect(await screen.findByRole("textbox", { name: "Arquivo README.md" })).toHaveTextContent("project-1: README.md");
});

it("keeps delayed file reads isolated when switching projects", async () => {
  const user = userEvent.setup();
  let finish: (value: FilePreview) => void = () => {};
  const original = mockedInvoke.getMockImplementation()!;
  mockedInvoke.mockImplementation((command, args, options) => command === "read_project_file" && (args as { projectId: string }).projectId === "project-1" ? new Promise(resolve => { finish = resolve; }) : original(command, args, options));
  const view = render(<Workspace />);
  await user.click(await screen.findByRole("treeitem", { name: "README.md" }));
  view.rerender(<Workspace projectId="project-2" />);
  await user.click(await screen.findByRole("treeitem", { name: "README.md" }));
  expect(await screen.findByRole("textbox", { name: "Arquivo README.md" })).toHaveTextContent("project-2");
  await act(async () => finish({ path: "README.md", content: "stale project-1", size: 10, encoding: "UTF-8" }));
  expect(screen.getByRole("textbox", { name: "Arquivo README.md" })).toHaveTextContent("project-2");
});

it("offers retry for unreadable files without affecting Chat", async () => {
  const user = userEvent.setup();
  const original = mockedInvoke.getMockImplementation()!;
  let fail = true;
  mockedInvoke.mockImplementation((command, args, options) => command === "read_project_file" && fail ? Promise.reject({ message: "Este arquivo é binário." }) : original(command, args, options));
  render(<Workspace />);
  await user.click(await screen.findByRole("treeitem", { name: "README.md" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Este arquivo é binário.");
  fail = false;
  await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
  await screen.findByRole("textbox", { name: "Arquivo README.md" });
  expect(screen.getByRole("tab", { name: "Chat" })).toBeEnabled();
});

it("loads folders only when expanded, supports keyboard navigation and refreshes the tree", async () => {
  const user = userEvent.setup();
  render(<Workspace />);
  const folder = await screen.findByRole("treeitem", { name: "src" });
  expect(mockedInvoke).not.toHaveBeenCalledWith("list_project_directory", { projectId: "project-1", path: "src" });
  folder.focus();
  await user.keyboard("{ArrowRight}");
  await screen.findByRole("treeitem", { name: "app.ts" });
  await user.keyboard("{ArrowDown}{Enter}");
  await screen.findByRole("textbox", { name: "Arquivo src/app.ts" });
  await user.click(screen.getByRole("button", { name: "Recolher pastas" }));
  expect(screen.queryByRole("treeitem", { name: "app.ts" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Atualizar Explorer" }));
  await waitFor(() => expect(mockedInvoke.mock.calls.filter(([command, args]) => command === "list_project_directory" && (args as { path: string }).path === "")).toHaveLength(2));
});
