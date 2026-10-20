import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DiagnosticLogSettings } from "./DiagnosticLogSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const status = { files: 3, bytes: 2 * 1024 * 1024, maxFiles: 8, maxBytes: 56 * 1024 * 1024, droppedRecordsCurrentRun: 0 };
beforeEach(() => { call.mockReset(); });

describe("diagnostic log maintenance", () => {
  it("shows bounded retention and a structural loading state without clearing automatically", async () => {
    let resolve: (value: typeof status) => void = () => {};
    call.mockImplementation(() => new Promise(yes => { resolve = yes; }));
    render(<DiagnosticLogSettings />);
    expect(screen.getByRole("status", { name: "Analisando logs internos" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Limpar logs" })).toBeDisabled();
    await act(async () => { resolve(status); });
    expect(screen.getByText(/56 MB em 8 arquivos/)).toBeInTheDocument();
    expect(screen.getByText(/3 arquivos/)).toBeInTheDocument();
    expect(call).toHaveBeenCalledExactlyOnceWith("get_diagnostic_log_status");
  });

  it("clears only after confirmation and allows cancellation without changing data", async () => {
    call.mockImplementation(async command => command === "get_diagnostic_log_status" ? status : { freedBytes: status.bytes, status: { ...status, files: 0, bytes: 0 } });
    const user = userEvent.setup();
    render(<DiagnosticLogSettings />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Limpar logs" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Limpar logs" }));
    let dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Conversas, projetos, configurações e incidentes compartilhados serão preservados");
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(call.mock.calls.filter(([command]) => command === "clear_diagnostic_logs")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Limpar logs" }));
    dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Confirmar limpeza" }));
    expect(call).toHaveBeenLastCalledWith("clear_diagnostic_logs", { confirmed: true });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Limpar logs" })).toBeDisabled();
  });

  it("does not replay a failed cleanup and requires a fresh analysis before another attempt", async () => {
    call.mockImplementation(async command => {
      if (command === "get_diagnostic_log_status") return status;
      throw { code: "diagnostic_error", message: "A limpeza pode estar parcial. Analise novamente." };
    });
    const user = userEvent.setup(); render(<DiagnosticLogSettings />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Limpar logs" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Limpar logs" }));
    await user.click(await screen.findByRole("button", { name: "Confirmar limpeza" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("parcial");
    expect(screen.getByRole("button", { name: "Limpar logs" })).toBeDisabled();
    expect(call.mock.calls.filter(([command]) => command === "clear_diagnostic_logs")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Analisar novamente" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Limpar logs" })).toBeEnabled());
    expect(call.mock.calls.filter(([command]) => command === "clear_diagnostic_logs")).toHaveLength(1);
  });

  it("keeps failure and dropped-record evidence visible", async () => {
    call.mockRejectedValueOnce(new Error("unavailable")).mockResolvedValue({ ...status, droppedRecordsCurrentRun: 4 });
    const user = userEvent.setup(); render(<DiagnosticLogSettings />);
    expect(await screen.findByRole("alert")).toHaveTextContent("unavailable");
    await user.click(screen.getByRole("button", { name: "Analisar novamente" }));
    expect(await screen.findByText(/4 registros de diagnóstico não puderam ser gravados/)).toBeInTheDocument();
  });
});
