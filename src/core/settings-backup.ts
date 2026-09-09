import { z } from "zod";
import { modelChoiceSchema } from "./workflow-catalog";

export const backupSummarySchema = z.object({
  customAgents: z.number().int().nonnegative(),
  customFlows: z.number().int().nonnegative(),
  skills: z.number().int().nonnegative(),
  mcps: z.number().int().nonnegative(),
  modelTargets: z.number().int().nonnegative(),
});

export const backupModelTargetSchema = z.object({
  id: z.string().min(1),
  kind: z.enum(["builtin_agent", "custom_agent"]),
  label: z.string().min(1),
  details: z.array(z.string()),
});

export const backupPreviewSchema = z.object({
  fingerprint: z.string().startsWith("sha256:"),
  createdAt: z.number().int().nonnegative(),
  appVersion: z.string().min(1),
  archiveBytes: z.number().int().nonnegative(),
  summary: backupSummarySchema,
  modelTargets: z.array(backupModelTargetSchema),
  warnings: z.array(z.string()),
});

export const backupExportResultSchema = z.object({
  path: z.string().min(1),
  bytes: z.number().int().nonnegative(),
  summary: backupSummarySchema,
});

export const backupImportResultSchema = z.object({
  summary: backupSummarySchema,
  mappedModels: z.number().int().nonnegative(),
});

export const backupModelMappingSchema = z.object({
  targetId: z.string().min(1),
  choice: modelChoiceSchema,
});

export type BackupPreview = z.infer<typeof backupPreviewSchema>;
export type BackupModelTarget = z.infer<typeof backupModelTargetSchema>;
export type BackupModelMapping = z.infer<typeof backupModelMappingSchema>;
export type BackupSummary = z.infer<typeof backupSummarySchema>;

export function formatBackupSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  const power = bytes > 0 ? Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1) : 0;
  return `${(bytes / 1024 ** power).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} ${units[power]}`;
}
