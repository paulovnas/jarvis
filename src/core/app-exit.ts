import { invoke } from "@tauri-apps/api/core";
import { appShutdownStatusSchema, type AppShutdownStatus } from "./app-update";

export async function getPendingAppExit(): Promise<AppShutdownStatus | null> {
  return appShutdownStatusSchema.nullable().parse(await invoke("get_pending_app_exit"));
}

export async function confirmAppExit(): Promise<void> { await invoke("confirm_app_exit"); }
export async function cancelAppExit(): Promise<void> { await invoke("cancel_app_exit"); }
