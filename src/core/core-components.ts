import { z } from "zod";

export const ACTIVE_CORE_IDS = ["context-mode", "ponytail", "beads", "open-design", "context7", "lsp", "openmontage", "comfyui", "graft"] as const;
export const coreIdSchema = z.enum([...ACTIVE_CORE_IDS, "hyperframes", "audiovisual"]);
export type CoreId = z.infer<typeof coreIdSchema>;
export const activeCore = (id: CoreId) => id !== "hyperframes" && id !== "audiovisual";
export const optionalCore = (id: CoreId) => id === "context7";
export const coreDownloadSchema = z.object({
  receivedBytes: z.number().int().nonnegative(), totalBytes: z.number().int().positive().nullable(),
});
export const coreDownloadEventSchema = z.object({ id: coreIdSchema, download: coreDownloadSchema });
export const coreSnapshotSchema = z.object({
  ready: z.boolean(), checking: z.boolean(),
  items: z.array(z.object({
    id: coreIdSchema, name: z.string(), repository: z.string().url(),
    installedVersion: z.string().nullable(), latestVersion: z.string().nullable(),
    installed: z.boolean(), configured: z.boolean(), updateAvailable: z.boolean(), stage: z.string().nullable(), error: z.string().nullable(),
    download: coreDownloadSchema.nullable(),
    healthError: z.string().nullable().default(null),
    diagnostics: z.array(z.object({ label: z.string(), passed: z.boolean(), message: z.string() })).default([]),
  })).refine(items => {
    const ids = new Set(items.map(item => item.id));
    if (ids.size !== items.length) return false;
    const expected = ids.has("openmontage") ? ACTIVE_CORE_IDS : [...ACTIVE_CORE_IDS.filter(id => id !== "openmontage"), "hyperframes", "audiovisual"];
    return ids.size === expected.length && expected.every(id => ids.has(id as CoreId));
  }),
}).refine(value => value.ready === value.items.every(item => optionalCore(item.id) || (item.installed && item.configured && !item.healthError)));
export type CoreSnapshot = z.infer<typeof coreSnapshotSchema>;
export function coreError(cause: unknown): string {
  const error = z.object({ message: z.string().min(1) }).safeParse(cause);
  return error.success ? error.data.message : "Não foi possível acessar o Core. Tente novamente.";
}
