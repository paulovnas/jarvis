import { Channel, invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { readResource } from "./resource-request";
import { version } from "../../package.json";

export const APP_VERSION = version;
export const PROJECT_URL = "https://github.com/paulovnas/jarvis";
export const updateInfoSchema = z.object({
  currentVersion: z.string(), installable: z.boolean(),
  available: z.object({ version: z.string(), notes: z.string(), publishedAt: z.string().nullable() }).nullable(),
});
export type UpdateInfo = z.infer<typeof updateInfoSchema>;
export const appShutdownStatusSchema = z.object({
  activeChats: z.number().int().nonnegative(),
  activeProcesses: z.number().int().nonnegative(),
  restartableProcesses: z.number().int().nonnegative(),
});
export type AppShutdownStatus = z.infer<typeof appShutdownStatusSchema>;
export type UpdateProgress = { stage: "downloading"; downloaded: number; total: number | null }
  | { stage: "verifying" | "installing" | "restarting" };

export function nativeUpdaterAvailable(): boolean { return "__TAURI_INTERNALS__" in window; }
export function displayVersion(value: string): string {
  return value.replace(/-beta\.(\d+)$/, (_, revision: string) => ` Beta${revision === "1" ? "" : ` ${revision}`}`);
}
export async function checkAppUpdate(): Promise<UpdateInfo> { return updateInfoSchema.parse(await readResource("check_app_update")); }
export async function getAppShutdownStatus(): Promise<AppShutdownStatus> { return appShutdownStatusSchema.parse(await invoke("get_app_shutdown_status")); }
export async function installAppUpdate(onProgress: (progress: UpdateProgress) => void, stopProcesses = false): Promise<void> {
  const channel = new Channel<UpdateProgress>();
  channel.onmessage = onProgress;
  await invoke("install_app_update", { onProgress: channel, ...(stopProcesses ? { stopProcesses: true } : {}) });
}
