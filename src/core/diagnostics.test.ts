import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearDiagnosticLogs, getDiagnosticLogStatus, installFrontendDiagnostics, reportFrontendDiagnostic } from "./diagnostics";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
let dispose = () => {};
beforeEach(() => { call.mockReset().mockResolvedValue(undefined); vi.stubGlobal("__TAURI_INTERNALS__", {}); });
afterEach(() => { dispose(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe("persistent frontend diagnostics", () => {
  it("captures window errors, promise rejections and React stacks without serializing context objects", () => {
    dispose = installFrontendDiagnostics();
    const error = new Error("frontend failed");
    window.dispatchEvent(new ErrorEvent("error", { error }));
    expect(call).toHaveBeenLastCalledWith("record_frontend_diagnostic", { event: { source: "window_error", level: "error", message: error.message, stack: error.stack } });
    const event = new Event("unhandledrejection");
    Object.defineProperty(event, "reason", { value: error });
    window.dispatchEvent(event);
    expect(call.mock.lastCall?.[1]).toMatchObject({ event: { source: "unhandled_rejection" } });
    reportFrontendDiagnostic("react_recovery", error, "\n at Settings");
    expect(call.mock.lastCall?.[1]).toMatchObject({ event: { level: "warning", stack: `${error.stack}\n\n at Settings` } });
    reportFrontendDiagnostic("react_error", { token: "private", prompt: "conversation" });
    expect(JSON.stringify(call.mock.lastCall)).not.toContain("private");
    expect(JSON.stringify(call.mock.lastCall)).not.toContain("conversation");
    reportFrontendDiagnostic("unhandled_rejection", { code: "native_error", message: "native request failed", stack: "native stack\nnext frame", token: "private" });
    expect(call.mock.lastCall?.[1]).toMatchObject({ event: { message: "native request failed", stack: "native stack\nnext frame" } });
    expect(JSON.stringify(call.mock.lastCall)).not.toContain("private");
  });

  it("preserves console behavior and removes listeners on disposal", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    dispose = installFrontendDiagnostics();
    console.warn("warning", { token: "private-context" });
    console.error(new Error("failure"));
    expect(warn).toHaveBeenCalledWith("warning", { token: "private-context" });
    expect(error).toHaveBeenCalledOnce();
    expect(JSON.stringify(call.mock.calls)).not.toContain("private-context");
    expect(call.mock.calls[0]?.[1]).toMatchObject({ event: { source: "console_warning", level: "warning" } });
    dispose();
    call.mockClear();
    console.warn("after disposal");
    expect(call).not.toHaveBeenCalled();
    expect(console.warn).toBe(warn);
  });

  it("does not invoke native logging in an ordinary browser and contains native logging failures", async () => {
    vi.unstubAllGlobals();
    dispose = installFrontendDiagnostics();
    reportFrontendDiagnostic("react_error", new Error("browser only"));
    expect(call).not.toHaveBeenCalled();
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    call.mockRejectedValue(new Error("logging unavailable"));
    reportFrontendDiagnostic("react_error", new Error("original"));
    await Promise.resolve();
    expect(call).toHaveBeenCalledOnce();
  });

  it("validates storage metrics and requires confirmation when clearing logs", async () => {
    const status = { files: 2, bytes: 200, maxFiles: 8, maxBytes: 56 * 1024 * 1024, droppedRecordsCurrentRun: 0 };
    call.mockResolvedValueOnce(status).mockResolvedValueOnce({ freedBytes: 200, status: { ...status, files: 0, bytes: 0 } });
    expect(await getDiagnosticLogStatus()).toEqual(status);
    expect((await clearDiagnosticLogs()).freedBytes).toBe(200);
    expect(call).toHaveBeenLastCalledWith("clear_diagnostic_logs", { confirmed: true });
    call.mockResolvedValue({ ...status, bytes: -1 });
    await expect(getDiagnosticLogStatus()).rejects.toThrow();
  });
});
