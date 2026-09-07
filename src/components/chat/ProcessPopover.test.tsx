import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { ChatProcess } from "@/core/processes";
import { ProcessPopover } from "./ProcessPopover";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const process: ChatProcess = { id: "service", conversationId: "chat", title: "Vite", command: "bun run dev", cwd: "/project", pid: 123, startedAt: 1, endedAt: null, exitCode: null, status: "running" };
beforeEach(() => vi.mocked(invoke).mockReset());
describe("Persistent processes", () => {
  it("hides the badge with no services and does not include another conversation", async () => {
    vi.mocked(invoke).mockResolvedValue([{ ...process, conversationId: "other" }]);
    render(<ProcessPopover conversationId="chat" />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("list_chat_processes", { conversationId: "chat" }));
    expect(screen.queryByRole("button", { name: /processos em execução/ })).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("read_chat_process", expect.anything());
  });
  it("shows service metadata, reads logs on demand and requires confirmation to stop", async () => {
    let stopped = false;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_processes") return [{ ...process, status: stopped ? "stopped" : "running" }];
      if (command === "read_chat_process") return { output: "Local: http://localhost:1420/" };
      if (command === "stop_chat_process") { stopped = true; return; }
    });
    const user = userEvent.setup(); render(<ProcessPopover conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "1 processos em execução" }));
    expect(screen.getByText("bun run dev")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Remover Vite" })).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("read_chat_process", expect.anything());
    await user.click(screen.getByRole("button", { name: "Saída" }));
    expect(await screen.findByLabelText("Saída de Vite")).toHaveTextContent("localhost:1420");
    await user.click(screen.getByRole("button", { name: "Parar Vite" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Parar Vite?");
    expect(invoke).not.toHaveBeenCalledWith("stop_chat_process", expect.anything());
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "1 processos em execução" }));
    await user.click(screen.getByRole("button", { name: "Parar Vite" }));
    await user.click(screen.getByRole("button", { name: "Parar processo" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("stop_chat_process", { conversationId: "chat", id: "service", confirmed: true }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "1 processos em execução" })).not.toBeInTheDocument());
  });
  it.each(["failed", "exited"] as const)("keeps a %s process accessible after closing and removes it without stopping anything", async status => {
    let removed = false;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_processes") return removed ? [] : [{ ...process, status, exitCode: status === "failed" ? 127 : 0 }];
      if (command === "read_chat_process") return { output: "/bin/bash: npm: No such file or directory" };
      if (command === "remove_chat_process") { removed = true; return; }
    });
    const user = userEvent.setup(); render(<ProcessPopover conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "1 processos encerrados" }));
    expect(screen.queryByRole("button", { name: "Parar Vite" })).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "1 processos encerrados" }));
    await user.click(screen.getByRole("button", { name: "Saída" }));
    expect(await screen.findByLabelText("Saída de Vite")).toHaveTextContent("No such file or directory");
    await user.click(screen.getByRole("button", { name: "Remover Vite" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "1 processos encerrados" })).not.toBeInTheDocument());
    expect(screen.queryByLabelText("Saída de Vite")).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("remove_chat_process", { conversationId: "chat", id: "service" });
    expect(invoke).not.toHaveBeenCalledWith("stop_chat_process", expect.anything());
  });
  it("keeps the failed record when removal fails and permits retry", async () => {
    let fail = true;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_processes") return fail ? [{ ...process, status: "failed" }] : [];
      if (command === "remove_chat_process" && fail) throw { message: "Falha ao remover." };
    });
    const user = userEvent.setup(); render(<ProcessPopover conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "1 processos encerrados" }));
    await user.click(screen.getByRole("button", { name: "Remover Vite" }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Falha ao remover."));
    expect(screen.getByText("Falhou")).toBeVisible();
    expect(screen.getByRole("button", { name: "Remover Vite" })).toBeEnabled();
    fail = false;
    await user.click(screen.getByRole("button", { name: "Remover Vite" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "1 processos encerrados" })).not.toBeInTheDocument());
  });
  it("removes a failed record without removing the other running service", async () => {
    let removed = false;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "list_chat_processes") return [process, ...removed ? [] : [{ ...process, id: "failed", title: "Falho", status: "failed" }]];
      if (command === "remove_chat_process") { removed = true; return; }
    });
    const user = userEvent.setup(); render(<ProcessPopover conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "1 processos em execução" }));
    await user.click(screen.getByRole("button", { name: "Remover Falho" }));
    await waitFor(() => expect(screen.queryByText("Falho")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Parar Vite" })).toBeVisible();
    expect(invoke).not.toHaveBeenCalledWith("stop_chat_process", expect.anything());
  });
  it("clears the previous conversation's cards immediately on navigation", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "list_chat_processes") return args && "conversationId" in args && args.conversationId === "chat" ? [{ ...process, status: "failed" }] : [];
    });
    const user = userEvent.setup(); const view = render(<ProcessPopover conversationId="chat" />);
    await user.click(await screen.findByRole("button", { name: "1 processos encerrados" }));
    view.rerender(<ProcessPopover conversationId="other" />);
    expect(screen.queryByRole("button", { name: "Remover Vite" })).not.toBeInTheDocument();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("list_chat_processes", { conversationId: "other" }));
    expect(screen.queryByRole("button", { name: "1 processos encerrados" })).not.toBeInTheDocument();
  });
});
