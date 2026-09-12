import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatCleanupSettings } from "./ChatCleanupSettings";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class<T> { onmessage?: (value: T) => void; },
}));
const call = vi.mocked(invoke);
const preview = { days: 7, bytes: 24000, protected: 1, conversations: [{ id: "old", title: "Conversa antiga", projectId: "p", projectName: "Projeto", activity: 100, bytes: 24000 }] };
const emptyCache = { bytes: 0, repositories: 0, residues: 0 };
const emptyJournals = { files: 0, conversationJournals: 0, workerJournals: 0, protectedFiles: 0, invalidFiles: 0, candidates: 0, currentBytes: 0, liveBytes: 0, recoverableBytes: 0, obsoleteRecords: 0, maxAmplificationBps: 100 };
describe("conversation cleanup", () => {
  beforeEach(() => { call.mockReset(); });
  it("previews seven-day cleanup and only deletes the reviewed IDs after confirmation", async () => {
    call.mockImplementation(async (command) => {
      if (command === "get_skill_cache_status") return emptyCache;
      if (command === "get_journal_maintenance_status") return emptyJournals;
      if (command === "preview_chat_cleanup") return preview;
      if (command === "cleanup_old_chats") return { deleted: 1, skipped: 0, failed: 0, bytes: 24000 };
      throw new Error(`unexpected command: ${command}`);
    });
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(call).toHaveBeenCalledWith("preview_chat_cleanup", { days: 7 });
    expect(dialog).toHaveTextContent("Conversa antiga");
    expect(dialog).toHaveTextContent("As pastas dos projetos serão preservadas");
    await user.click(within(dialog).getByRole("button", { name: "Excluir conversas" }));
    expect(call).toHaveBeenLastCalledWith("cleanup_old_chats", { days: 7, selection: [{ id: "old", activity: 100 }], confirmed: true });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });
  it("allows cancelling without deleting and shows an empty analysis", async () => {
    let analyses = 0;
    call.mockImplementation(async (command) => {
      if (command === "get_skill_cache_status") return emptyCache;
      if (command === "get_journal_maintenance_status") return emptyJournals;
      if (command === "preview_chat_cleanup") return analyses++ === 0 ? preview : { ...preview, conversations: [] };
      throw new Error(`unexpected command: ${command}`);
    });
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    await user.click(await screen.findByRole("button", { name: "Cancelar" }));
    expect(call.mock.calls.filter(([command]) => command === "cleanup_old_chats")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Nenhuma conversa disponível");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
  it("uses a skeleton during analysis and exposes a retry after failure", async () => {
    let reject: (error: Error) => void = () => {};
    call.mockImplementation((command) => {
      if (command === "get_skill_cache_status") return Promise.resolve(emptyCache);
      if (command === "get_journal_maintenance_status") return Promise.resolve(emptyJournals);
      if (command === "preview_chat_cleanup") return new Promise((_resolve, no) => { reject = no; });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    await user.click(screen.getByRole("button", { name: "Revisar limpeza" }));
    expect(screen.getByRole("status", { name: "Analisando históricos" })).toBeInTheDocument();
    reject(new Error("offline"));
    expect(await screen.findByRole("alert")).toHaveTextContent("offline");
    expect(screen.getByRole("button", { name: "Revisar limpeza" })).toBeEnabled();
  });
  it("shows repository cache usage and clears it only after confirmation", async () => {
    const cache = { bytes: 2 * 1024 * 1024 * 1024, repositories: 4, residues: 2 };
    call.mockImplementation(async (command) => {
      if (command === "get_skill_cache_status") return cache;
      if (command === "get_journal_maintenance_status") return emptyJournals;
      if (command === "clear_skill_cache") return { freedBytes: cache.bytes, removedRepositories: 4, removedResidues: 2, status: emptyCache };
      throw new Error(`unexpected command: ${command}`);
    });
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    const card = screen.getByRole("region", { name: "Cache de recursos" });
    expect(await within(card).findByText("2 GB")).toBeInTheDocument();
    expect(within(card).getByText("4 repositórios · 2 resíduos antigos")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Limpar cache" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(call.mock.calls.filter(([command]) => command === "clear_skill_cache")).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Limpar cache" }));
    expect(call).toHaveBeenCalledWith("clear_skill_cache", { confirmed: true });
    await waitFor(() => expect(within(card).getByText("0 B")).toBeInTheDocument());
  });
  it("shows recoverable journal space and reports background compaction progress", async () => {
    const status = { ...emptyJournals, files: 3, conversationJournals: 2, workerJournals: 1, candidates: 2, currentBytes: 12 * 1024 * 1024, liveBytes: 4 * 1024 * 1024, recoverableBytes: 8 * 1024 * 1024, obsoleteRecords: 519, maxAmplificationBps: 420 };
    let complete: () => void = () => {};
    call.mockImplementation((command, args) => {
      if (command === "get_skill_cache_status") return Promise.resolve(emptyCache);
      if (command === "get_journal_maintenance_status") return Promise.resolve(status);
      if (command === "optimize_journals") {
        const channel = (args as { onProgress: { onmessage?: (value: unknown) => void } }).onProgress;
        channel.onmessage?.({ phase: "compacting", processedFiles: 1, totalFiles: 2, recoveredBytes: 4 * 1024 * 1024, currentKind: "worker" });
        return new Promise(resolve => { complete = () => resolve({ optimizedFiles: 2, failedFiles: 0, recoveredBytes: status.recoverableBytes, status: emptyJournals }); });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup(); render(<ChatCleanupSettings />);
    const card = screen.getByRole("region", { name: "Otimização dos históricos" });
    expect(await within(card).findByText("8 MB")).toBeInTheDocument();
    expect(within(card).getByText("4,2×")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Otimizar históricos" }));
    expect(within(card).getByRole("status", { name: "Progresso da otimização dos históricos" })).toHaveTextContent("1/2 · 4 MB");
    complete();
    await waitFor(() => expect(within(card).getByRole("button", { name: "Otimizar históricos" })).toBeDisabled());
  });
});
