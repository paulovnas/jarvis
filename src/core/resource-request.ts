import { invoke, type InvokeArgs } from "@tauri-apps/api/core";

// Only reads/discovery belong here. Timing out IPC does not cancel a native
// mutation; installs, OAuth consent and agent execution keep their own lifecycle.
const deadlines = {
  get_app_config: 15_000,
  get_core_status: 15_000,
  list_provider_accounts: 60_000,
  list_skills: 15_000,
  get_library_snapshot: 15_000,
  get_provider_usage: 45_000,
  get_claude_usage: 45_000,
  check_core_updates: 40_000,
  check_app_update: 45_000,
  refresh_provider_models: 120_000,
  browse_skill_marketplace: 30_000,
  get_marketplace_skill: 120_000,
  get_skill_detail: 15_000,
  check_skill_updates: 120_000,
  test_mcp_server: 100_000,
  lookup_custom_model: 30_000,
} as const;

export class ResourceTimeoutError extends Error {
  readonly code = "resource_timeout";
  constructor() {
    super("A consulta demorou demais. Verifique a conexão e tente novamente.");
  }
}

export async function readResource<T = unknown>(command: keyof typeof deadlines, args?: InvokeArgs): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const timeout = command === "list_provider_accounts" && args && "cached" in args && args.cached === true ? 15_000 : deadlines[command];
    return await Promise.race([
      args === undefined ? invoke<T>(command) : invoke<T>(command, args),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new ResourceTimeoutError()), timeout); }),
    ]);
  } finally { clearTimeout(timer); }
}

/** Retry failed reads on connectivity/focus changes, with a fallback for Wi-Fi
 * changes that the WebView does not report. Callers guard in-flight requests. */
export function watchResourceRecovery(retryFailed: () => void): () => void {
  let attemptedAt = -Infinity;
  const retry = () => {
    if (navigator.onLine === false || document.visibilityState === "hidden" || Date.now() - attemptedAt < 5_000) return;
    attemptedAt = Date.now();
    retryFailed();
  };
  window.addEventListener("online", retry);
  window.addEventListener("focus", retry);
  document.addEventListener("visibilitychange", retry);
  const timer = setInterval(retry, 60_000);
  return () => {
    clearInterval(timer);
    window.removeEventListener("online", retry);
    window.removeEventListener("focus", retry);
    document.removeEventListener("visibilitychange", retry);
  };
}
