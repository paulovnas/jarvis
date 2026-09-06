import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatProcess } from "@/core/processes";
import { ProcessPopover } from "./ProcessPopover";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
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
});
