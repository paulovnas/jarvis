import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FileChange } from "@/core/chat";
import { ChangedFiles } from "./ChangedFiles";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
const call = vi.mocked(invoke);
const open = vi.mocked(openPath);
const files: FileChange[] = Array.from({ length: 8 }, (_, index) => ({ path: `src/file-${index}.ts`, additions: index + 1, deletions: 1, base: "conversation" }));
const diff = (path: string) => ({ path, base: "conversation", truncated: false, rows: [{ kind: "removed", oldLine: 1, newLine: null, text: "const before = 1;" }, { kind: "added", oldLine: null, newLine: 1, text: `const after = '${path}';` }] });

describe("Changed file review", () => {
  beforeEach(() => { call.mockReset().mockImplementation(async (_command, args) => diff((args as { path: string }).path)); open.mockReset().mockResolvedValue(); });

  it("limits preview to five, shows totals and opens the clicked file in the diff dialog", async () => {
    const user = userEvent.setup(); render(<ChangedFiles conversationId="c1" files={files} />);
    expect(screen.getAllByRole("button", { name: /Alterações em/ })).toHaveLength(5);
    expect(screen.getByLabelText("36 linhas adicionadas, 8 linhas removidas")).toBeInTheDocument();
    expect(screen.queryByText("file-6.ts")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Alterações em src/file-2.ts" }));
    const dialog = await screen.findByRole("dialog", { name: "Arquivos alterados" });
    expect(within(dialog).getAllByRole("button", { name: /Alterações em/ })).toHaveLength(8);
    expect(within(dialog).getByRole("button", { name: "Alterações em src/file-2.ts" })).toHaveAttribute("aria-pressed", "true");
    expect(await within(dialog).findByText("const before = 1;")).toBeInTheDocument();
    expect(within(dialog).getByText("const after = 'src/file-2.ts';")).toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("get_agent_file_diff", { conversationId: "c1", path: "src/file-2.ts" });
    await user.click(within(dialog).getByRole("button", { name: "Alterações em src/file-7.ts" }));
    expect(await within(dialog).findByText("const after = 'src/file-7.ts';")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Fechar alterações" }));
    await user.click(screen.getByRole("button", { name: "Ver tudo (8)" }));
    expect(await screen.findByText("const after = 'src/file-0.ts';")).toBeInTheDocument();
  });

  it("ignores a slow previous diff when the file selection changes", async () => {
    const user = userEvent.setup(); let resolve!: (value: unknown) => void;
    call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    render(<ChangedFiles conversationId="c1" files={files} />);
    await user.click(screen.getByRole("button", { name: "Alterações em src/file-0.ts" }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Alterações em src/file-1.ts" }));
    expect(await screen.findByText("const after = 'src/file-1.ts';")).toBeInTheDocument();
    await act(async () => resolve(diff("src/file-0.ts")));
    expect(screen.queryByText("const after = 'src/file-0.ts';")).not.toBeInTheDocument();
  });

  it("reports failed diffs, retries and labels incomplete legacy baselines", async () => {
    const user = userEvent.setup(); call.mockRejectedValueOnce({ message: "Arquivo indisponível" });
    render(<ChangedFiles conversationId="c1" files={[{ ...files[0], base: "unknown", additions: null, deletions: null }]} />);
    expect(screen.getByText("Sem base")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Alterações em/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Arquivo indisponível");
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(await screen.findByText("const before = 1;")).toBeInTheDocument();
  });

  it("opens the project folder and selected file with the system defaults", async () => {
    const user = userEvent.setup(); render(<ChangedFiles conversationId="c1" projectPath="/projects/jarvis" files={files} />);
    await user.click(screen.getByRole("button", { name: "Alterações em src/file-2.ts" }));
    const dialog = await screen.findByRole("dialog", { name: "Arquivos alterados" });
    await user.click(within(dialog).getByRole("button", { name: "Abrir pasta do projeto" }));
    expect(open).toHaveBeenCalledWith("/projects/jarvis");
    await user.click(within(dialog).getByRole("button", { name: "Abrir src/file-2.ts no editor padrão" }));
    expect(open).toHaveBeenCalledWith("/projects/jarvis/src/file-2.ts");
  });
});
