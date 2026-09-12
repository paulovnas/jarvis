import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { DiagnosticEvidence } from "./DiagnosticEvidence";

const { invokeMock, saveMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  saveMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: saveMock }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const summary = {
  runId: "0123456789abcdef0123456789abcdef",
  appVersion: "0.9.10-beta",
  os: "macos",
  arch: "aarch64",
  startedAt: 1_757_376_000_000,
  logFiles: 2,
  logBytes: 2048,
  eventCount: 4,
  recentEvents: [{
    timestamp: 1_757_376_001_000,
    level: "error",
    event: "provider_refusal",
    provider: "custom",
    category: "provider_limit",
    httpStatus: 429,
    requestId: "req-safe",
  }],
  copyable: "Diagnóstico do Jarvis\nVersão: 0.9.10-beta\nrequest=req-safe",
};

const integrity = {
  healthy: true,
  message: "A estrutura do banco de dados está íntegra.",
  details: ["ok"],
  durationMs: 7,
  journalMode: "delete",
  busyTimeoutMs: 5000,
};

describe("diagnostic evidence", () => {
  beforeEach(() => {
    invokeMock.mockReset().mockImplementation((command: string) => Promise.resolve(
      command === "get_diagnostic_summary" ? summary
        : command === "check_database_integrity" ? integrity
          : { path: "/tmp/Jarvis-diagnostico.zip", bytes: 4096, events: 4 },
    ));
    saveMock.mockReset();
    vi.mocked(toast.success).mockReset();
    vi.mocked(toast.error).mockReset();
  });

  it("shows the latest incident and copies the sanitized summary", async () => {
    const user = userEvent.setup();
    const copy = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    render(<DiagnosticEvidence />);

    expect(await screen.findByText("Solicitação recusada pelo provedor")).toBeVisible();
    expect(screen.getByText("4 eventos · 2 KB")).toBeVisible();
    expect(screen.getByText("macos · aarch64")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Copiar diagnóstico" }));

    expect(copy).toHaveBeenCalledWith(summary.copyable);
    expect(toast.success).toHaveBeenCalledWith("Diagnóstico copiado");
  });

  it("exports a ZIP to the user-selected destination", async () => {
    const user = userEvent.setup();
    saveMock.mockResolvedValue("/tmp/Jarvis-diagnostico.zip");
    render(<DiagnosticEvidence />);
    await screen.findByText("0.9.10-beta");
    await user.click(screen.getByRole("button", { name: "Exportar pacote" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("export_diagnostic_bundle", { path: "/tmp/Jarvis-diagnostico.zip" }));
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({
      filters: [{ name: "Diagnóstico do Jarvis", extensions: ["zip"] }],
    }));
    expect(toast.success).toHaveBeenCalledWith("Pacote de diagnóstico salvo", {
      description: "4 KB · 4 eventos",
    });
  });

  it("checks SQLite integrity only when the user requests it", async () => {
    const user = userEvent.setup();
    render(<DiagnosticEvidence />);
    await screen.findByText("0.9.10-beta");

    expect(invokeMock).not.toHaveBeenCalledWith("check_database_integrity");
    await user.click(screen.getByRole("button", { name: "Verificar banco" }));

    expect(await screen.findByText("A estrutura do banco de dados está íntegra.")).toBeVisible();
    expect(screen.getByText("Modo DELETE · espera de 5s · 7ms")).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith("check_database_integrity");
    expect(toast.success).toHaveBeenCalledWith("Banco de dados íntegro");
  });

  it("keeps core repair usable when local evidence cannot be read", async () => {
    invokeMock.mockRejectedValue({ message: "Registros indisponíveis." });
    render(<DiagnosticEvidence />);

    expect(await screen.findByRole("alert")).toHaveTextContent("Registros indisponíveis.");
    expect(screen.getByRole("button", { name: "Copiar diagnóstico" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Exportar pacote" })).toBeDisabled();
  });
});
