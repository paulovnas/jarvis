import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { Hook, HookCatalog } from "@/core/hooks";
import { HooksSettings } from "./HooksSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const mocked = vi.mocked(invoke);
const hook: Hook = { id: "a".repeat(32), name: "Formatar arquivos", event: "PostToolUse", command: "bun run format", matcher: "write|edit", timeoutSeconds: 60, enabled: true };
const initial: HookCatalog = { revision: 3, hooks: [hook], untrustedIds: [], nativeHooks: [{ id: "native-context", name: "Memória da sessão", event: "SessionStart", description: "Restaura a memória auxiliar.", command: "node jarvis-hook.mjs", matcher: null, timeoutSeconds: 3 }, { id: "native-policy", name: "Política do agente", event: "BeforeAgent", description: "Preserva as instruções do projeto.", command: null, matcher: null, timeoutSeconds: null }] };
let changed: EventCallback<unknown> | undefined;
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }

describe("HooksSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks(); changed = undefined;
    vi.mocked(listen).mockImplementation(async (event, callback) => { if (event === "hooks:changed") changed = callback; return () => { changed = undefined; }; });
    mocked.mockReset().mockImplementation(async command => { if (command === "list_hooks") return initial; throw new Error("Unexpected command"); });
  });

  it("groups actual events and keeps native hooks strictly read-only", async () => {
    const user = userEvent.setup(); render(<HooksSettings />);
    await screen.findByRole("button", { name: "Detalhes do hook Memória da sessão" });
    expect(screen.getByRole("heading", { name: "Início da sessão" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Após usar a ferramenta" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Editar hook Memória da sessão" })).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Ativar hook Memória da sessão" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Detalhes do hook Política do agente" }));
    expect(screen.getAllByText("Interno do Jarvis")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "Remover hook Política do agente" })).not.toBeInTheDocument();
    expect(mocked).toHaveBeenCalledExactlyOnceWith("list_hooks");
    const edit = screen.getByRole("button", { name: "Editar hook Formatar arquivos" });
    expect(edit.nextElementSibling).toBe(screen.getByRole("switch", { name: "Ativar hook Formatar arquivos" }));
  });

  it("creates a hook after explicit save and preserves literal commands", async () => {
    const user = userEvent.setup(); render(<HooksSettings />);
    await screen.findByRole("button", { name: "Editar hook Formatar arquivos" });
    await user.click(screen.getByRole("button", { name: "Adicionar hook" }));
    const dialog = screen.getByRole("dialog", { name: "Adicionar hook" });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Nome do hook" }), { target: { value: "Validar gravação" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Comando" }), { target: { value: "  node validate.mjs\n" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Matcher (opcional)" }), { target: { value: "write|edit" } });
    expect(mocked).not.toHaveBeenCalledWith("save_hook", expect.anything());
    mocked.mockImplementationOnce(async (command, args) => { expect(command).toBe("save_hook"); const request = args as { hook: Hook; expectedRevision: number }; expect(request.expectedRevision).toBe(3); expect(request.hook).toMatchObject({ name: "Validar gravação", command: "  node validate.mjs\n", matcher: "write|edit", timeoutSeconds: 600, event: "PreToolUse", enabled: true }); expect(request.hook.id).toMatch(/^[a-f0-9]{32}$/); return { ...initial, revision: 4, hooks: [...initial.hooks, request.hook] }; });
    await user.click(within(dialog).getByRole("button", { name: "Salvar hook" }));
    expect(await screen.findByRole("button", { name: "Editar hook Validar gravação" })).toBeVisible();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(toast.success).toHaveBeenCalledWith("Hook salvo");
  });

  it("edits a manual hook and retains unsaved input after storage failure", async () => {
    const user = userEvent.setup(); render(<HooksSettings />);
    await user.click(await screen.findByRole("button", { name: "Editar hook Formatar arquivos" }));
    const command = screen.getByRole("textbox", { name: "Comando" });
    fireEvent.change(command, { target: { value: "bun run lint" } });
    mocked.mockRejectedValueOnce({ code: "hook_error", message: "Não foi possível gravar o catálogo." });
    await user.click(screen.getByRole("button", { name: "Salvar hook" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("gravar o catálogo");
    expect(command).toHaveValue("bun run lint");
    mocked.mockResolvedValueOnce({ ...initial, revision: 4, hooks: [{ ...hook, command: "bun run lint" }] });
    await user.click(screen.getByRole("button", { name: "Salvar hook" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mocked).toHaveBeenLastCalledWith("save_hook", { hook: { ...hook, command: "bun run lint" }, expectedRevision: 3 });
  });

  it("validates required fields without invoking a mutation", async () => {
    const user = userEvent.setup(); render(<HooksSettings />);
    await screen.findByRole("button", { name: "Editar hook Formatar arquivos" });
    await user.click(screen.getByRole("button", { name: "Adicionar hook" }));
    await user.click(screen.getByRole("button", { name: "Salvar hook" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Confira nome");
    expect(screen.getByRole("textbox", { name: "Nome do hook" })).toHaveAttribute("aria-invalid", "true");
    expect(mocked).toHaveBeenCalledExactlyOnceWith("list_hooks");
  });

  it("waits for a toggle to persist and blocks duplicate mutations", async () => {
    const pending = deferred<HookCatalog>();
    const user = userEvent.setup(); const busyChanged = vi.fn(); render(<HooksSettings onBusyChange={busyChanged} />);
    const control = await screen.findByRole("switch", { name: "Ativar hook Formatar arquivos" });
    mocked.mockReturnValueOnce(pending.promise);
    await user.click(control);
    expect(control).toHaveAttribute("aria-disabled", "true"); expect(control).toBeChecked();
    expect(screen.getByRole("button", { name: "Adicionar hook" })).toBeDisabled();
    expect(busyChanged).toHaveBeenLastCalledWith(true);
    await user.click(control);
    expect(mocked).toHaveBeenCalledTimes(2);
    await act(async () => pending.resolve({ ...initial, revision: 4, hooks: [{ ...hook, enabled: false }] }));
    expect(control).not.toBeChecked(); expect(control).not.toHaveAttribute("aria-disabled", "true");
    expect(mocked).toHaveBeenLastCalledWith("save_hook", { hook: { ...hook, enabled: false }, expectedRevision: 3 });
    expect(busyChanged).toHaveBeenLastCalledWith(false);
  });

  it("does not show a false disabled state when the toggle fails", async () => {
    const user = userEvent.setup(); render(<HooksSettings />);
    const control = await screen.findByRole("switch", { name: "Ativar hook Formatar arquivos" });
    mocked.mockRejectedValueOnce({ code: "hook_error", message: "Catálogo alterado; revise novamente." });
    await user.click(control);
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Catálogo alterado; revise novamente."));
    expect(control).toBeChecked(); expect(control).not.toHaveAttribute("aria-disabled", "true");
  });

  it("marks externally changed hooks for review and saves explicit approval before enabling", async () => {
    mocked.mockResolvedValueOnce({ ...initial, untrustedIds: [hook.id] });
    const user = userEvent.setup(); render(<HooksSettings />);
    const control = await screen.findByRole("switch", { name: "Revisar e ativar hook Formatar arquivos" });
    expect(control).not.toBeChecked();
    expect(screen.getByText("Revisão necessária")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Detalhes do hook Formatar arquivos" }));
    expect(screen.getByText(/alterado fora do Jarvis/)).toBeVisible();
    await user.click(control);
    const dialog = screen.getByRole("dialog", { name: "Editar hook" });
    expect(within(dialog).getByRole("textbox", { name: "Comando" })).toHaveValue(hook.command);
    expect(mocked).not.toHaveBeenCalledWith("save_hook", expect.anything());
    mocked.mockResolvedValueOnce({ ...initial, revision: 4 });
    await user.click(within(dialog).getByRole("button", { name: "Salvar hook" }));
    expect(mocked).toHaveBeenLastCalledWith("save_hook", { hook, expectedRevision: 3 });
    expect(await screen.findByRole("switch", { name: "Ativar hook Formatar arquivos" })).toBeChecked();
    expect(screen.queryByText("Revisão necessária")).not.toBeInTheDocument();
  });

  it("requires explicit removal and preserves the hook when cancelled", async () => {
    const user = userEvent.setup(); render(<HooksSettings />);
    await user.click(await screen.findByRole("button", { name: "Detalhes do hook Formatar arquivos" }));
    await user.click(screen.getByRole("button", { name: "Remover hook Formatar arquivos" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Cancelar" }));
    expect(mocked).toHaveBeenCalledExactlyOnceWith("list_hooks");
    await user.click(screen.getByRole("button", { name: "Remover hook Formatar arquivos" }));
    mocked.mockResolvedValueOnce({ ...initial, revision: 4, hooks: [] });
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Remover hook" }));
    expect(await screen.findByText("Nenhum hook manual")).toBeVisible();
    expect(mocked).toHaveBeenLastCalledWith("delete_hook", { id: hook.id, expectedRevision: 3 });
  });

  it("refreshes agent changes while retaining the editor revision and unsaved draft", async () => {
    const user = userEvent.setup(); const { unmount } = render(<HooksSettings />);
    await user.click(await screen.findByRole("button", { name: "Editar hook Formatar arquivos" }));
    const command = screen.getByRole("textbox", { name: "Comando" });
    fireEvent.change(command, { target: { value: "bun run check" } });
    mocked.mockResolvedValueOnce({ ...initial, revision: 4, hooks: [{ ...hook, name: "Nome atualizado" }] });
    act(() => changed?.({ event: "hooks:changed", id: 1, payload: null }));
    await waitFor(() => expect(mocked).toHaveBeenCalledTimes(2));
    expect(command).toHaveValue("bun run check");
    mocked.mockRejectedValueOnce({ code: "hook_error", message: "Revise a alteração concorrente." });
    await user.click(screen.getByRole("button", { name: "Salvar hook" }));
    expect(mocked).toHaveBeenLastCalledWith("save_hook", { hook: { ...hook, command: "bun run check" }, expectedRevision: 3 });
    expect(await screen.findByRole("alert")).toHaveTextContent("alteração concorrente");
    expect(command).toHaveValue("bun run check");
    unmount(); expect(changed).toBeUndefined();
  });
});
