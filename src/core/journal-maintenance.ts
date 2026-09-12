import { Channel, invoke } from "@tauri-apps/api/core";
import { z } from "zod";

export const journalMaintenanceSummarySchema = z.object({
  files: z.number().int().nonnegative(),
  conversationJournals: z.number().int().nonnegative(),
  workerJournals: z.number().int().nonnegative(),
  protectedFiles: z.number().int().nonnegative(),
  invalidFiles: z.number().int().nonnegative(),
  candidates: z.number().int().nonnegative(),
  currentBytes: z.number().nonnegative(),
  liveBytes: z.number().nonnegative(),
  recoverableBytes: z.number().nonnegative(),
  obsoleteRecords: z.number().int().nonnegative(),
  maxAmplificationBps: z.number().int().min(100),
});

export const journalMaintenanceProgressSchema = z.object({
  phase: z.enum(["analyzing", "compacting", "completed"]),
  processedFiles: z.number().int().nonnegative(),
  totalFiles: z.number().int().nonnegative(),
  recoveredBytes: z.number().nonnegative(),
  currentKind: z.enum(["conversation", "worker"]).nullable(),
});

export const journalMaintenanceResultSchema = z.object({
  optimizedFiles: z.number().int().nonnegative(),
  failedFiles: z.number().int().nonnegative(),
  recoveredBytes: z.number().nonnegative(),
  status: journalMaintenanceSummarySchema,
});

export type JournalMaintenanceSummary = z.infer<typeof journalMaintenanceSummarySchema>;
export type JournalMaintenanceProgress = z.infer<typeof journalMaintenanceProgressSchema>;

export async function getJournalMaintenanceStatus(): Promise<JournalMaintenanceSummary> {
  return journalMaintenanceSummarySchema.parse(await invoke("get_journal_maintenance_status"));
}

export async function optimizeJournals(
  onProgress: (progress: JournalMaintenanceProgress) => void,
) {
  const channel = new Channel<unknown>();
  channel.onmessage = (value) => onProgress(journalMaintenanceProgressSchema.parse(value));
  return journalMaintenanceResultSchema.parse(
    await invoke("optimize_journals", { onProgress: channel }),
  );
}
