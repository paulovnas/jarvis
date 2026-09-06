import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatCleanupSettings } from "./ChatCleanupSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const preview = { days: 7, bytes: 24000, protected: 1, conversations: [{ id: "old", title: "Conversa antiga", projectId: "p", projectName: "Projeto", activity: 100, bytes: 24000 }] };
describe("conversation cleanup", () => {
  beforeEach(() => call.mockReset());
  it("previews seven-day cleanup and only deletes the reviewed IDs after confirmation", async () => {
    call.mockResolvedValueOnce(preview).mockResolvedValueOnce({ deleted: 1, skipped: 0, failed: 0, bytes: 24000 });
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    expect(call).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(call).toHaveBeenCalledWith("preview_chat_cleanup", { days: 7 });
    expect(dialog).toHaveTextContent("Conversa antiga");
    expect(dialog).toHaveTextContent("As pastas dos projetos serão preservadas");
    expect(call).toHaveBeenCalledTimes(1);
    await user.click(within(dialog).getByRole("button", { name: "Excluir conversas" }));
    expect(call).toHaveBeenLastCalledWith("cleanup_old_chats", { days: 7, selection: [{ id: "old", activity: 100 }], confirmed: true });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });
  it("allows cancelling without deleting and shows an empty analysis", async () => {
    call.mockResolvedValueOnce(preview).mockResolvedValueOnce({ ...preview, conversations: [] });
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    await user.click(await screen.findByRole("button", { name: "Cancelar" }));
    expect(call).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Nenhuma conversa disponível");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
  it("uses a skeleton during analysis and exposes a retry after failure", async () => {
    let reject: (error: Error) => void = () => {};
    call.mockReturnValueOnce(new Promise((_resolve, no) => { reject = no; }));
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    expect(screen.getByRole("status", { name: "Analisando históricos" })).toBeInTheDocument();
    reject(new Error("offline"));
    expect(await screen.findByRole("alert")).toHaveTextContent("offline");
    expect(screen.getByRole("button", { name: "Revisar limpeza" })).toBeEnabled();
  });
});
