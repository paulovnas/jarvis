import { z } from "zod";

export const coreIdSchema = z.enum(["context-mode", "ponytail", "beads", "open-design"]);
export type CoreId = z.infer<typeof coreIdSchema>;
export const coreDownloadSchema = z.object({
  receivedBytes: z.number().int().nonnegative(), totalBytes: z.number().int().positive().nullable(),
});
export const coreDownloadEventSchema = z.object({ id: coreIdSchema, download: coreDownloadSchema });
export const coreSnapshotSchema = z.object({
  ready: z.boolean(), checking: z.boolean(),
  items: z.array(z.object({
    id: coreIdSchema, name: z.string(), repository: z.string().url(),
    installedVersion: z.string().nullable(), latestVersion: z.string().nullable(),
    installed: z.boolean(), updateAvailable: z.boolean(), stage: z.string().nullable(), error: z.string().nullable(),
    download: coreDownloadSchema.nullable(),
  })).length(4).refine(items => new Set(items.map(item => item.id)).size === 4),
}).refine(value => value.ready === value.items.every(item => item.installed));
export type CoreSnapshot = z.infer<typeof coreSnapshotSchema>;
export function coreError(cause: unknown): string {
  const error = z.object({ message: z.string().min(1) }).safeParse(cause);
  return error.success ? error.data.message : "Não foi possível acessar o Core. Tente novamente.";
}
