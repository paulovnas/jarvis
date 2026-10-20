import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";

export const diagnosticLogStatusSchema = z.object({
  files: z.number().int().nonnegative(),
  bytes: z.number().int().nonnegative(),
  maxFiles: z.number().int().positive(),
  maxBytes: z.number().int().positive(),
  droppedRecordsCurrentRun: z.number().int().nonnegative(),
}).strict();
export type DiagnosticLogStatus = z.infer<typeof diagnosticLogStatusSchema>;
const cleanupSchema = z.object({ freedBytes: z.number().int().nonnegative(), status: diagnosticLogStatusSchema }).strict();
export const getDiagnosticLogStatus = async () => diagnosticLogStatusSchema.parse(await invoke("get_diagnostic_log_status"));
export const clearDiagnosticLogs = async () => cleanupSchema.parse(await invoke("clear_diagnostic_logs", { confirmed: true }));

type Source = "window_error" | "unhandled_rejection" | "console_error" | "console_warning" | "react_error" | "react_recovery";

export function reportFrontendDiagnostic(source: Source, cause: unknown, componentStack?: string | null) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const detail = typeof cause === "object" && cause !== null ? cause as { message?: unknown; stack?: unknown } : null;
  const message = typeof detail?.message === "string" ? detail.message : typeof cause === "string" ? cause : "Erro de interface sem detalhes.";
  const stack = [typeof detail?.stack === "string" ? detail.stack : undefined, componentStack].filter(Boolean).join("\n") || null;
  // Do not serialize arbitrary objects: they may contain conversation data.
  void invoke("record_frontend_diagnostic", {
    event: { source, level: source === "console_warning" || source === "react_recovery" ? "warning" : "error", message, stack },
  }).catch(() => { /* Logging must not recursively produce another error. */ });
}

export function installFrontendDiagnostics() {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const error = (event: ErrorEvent) => reportFrontendDiagnostic("window_error", event.error instanceof Error ? event.error : event.message);
  const rejection = (event: PromiseRejectionEvent) => reportFrontendDiagnostic("unhandled_rejection", event.reason);
  window.addEventListener("error", error);
  window.addEventListener("unhandledrejection", rejection);
  const originalWarn = console.warn;
  const originalError = console.error;
  const wrap = (original: typeof console.warn, source: Source) => (...args: unknown[]) => {
    original.apply(console, args);
    const cause = args.find((arg): arg is Error => arg instanceof Error)
      ?? args.find((arg): arg is string => typeof arg === "string")
      ?? "Diagnóstico de interface sem detalhes.";
    reportFrontendDiagnostic(source, cause instanceof Error ? cause : new Error(cause));
  };
  const warn = wrap(originalWarn, "console_warning");
  const logError = wrap(originalError, "console_error");
  console.warn = warn;
  console.error = logError;
  return () => {
    window.removeEventListener("error", error);
    window.removeEventListener("unhandledrejection", rejection);
    if (console.warn === warn) console.warn = originalWarn;
    if (console.error === logError) console.error = originalError;
  };
}
