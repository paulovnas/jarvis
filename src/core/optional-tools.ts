import { z } from "zod";

export const optionalToolIdSchema = z.enum(["git", "gh"]);
export type OptionalToolId = z.infer<typeof optionalToolIdSchema>;

export const optionalToolsSnapshotSchema = z.object({
  platform: z.enum(["macos", "windows", "linux", "other"]),
  platformLabel: z.string().min(1),
  tools: z.array(z.object({
    id: optionalToolIdSchema,
    name: z.string().min(1),
    description: z.string().min(1),
    installed: z.boolean(),
    version: z.string().min(1).nullable(),
    automaticInstall: z.boolean(),
    installWith: z.string().min(1).nullable(),
    helpUrl: z.string().url(),
  })).length(2).refine(tools => new Set(tools.map(tool => tool.id)).size === 2),
});

export type OptionalTool = z.infer<typeof optionalToolsSnapshotSchema>["tools"][number];
export type OptionalToolsSnapshot = z.infer<typeof optionalToolsSnapshotSchema>;

export function optionalToolsError(cause: unknown, fallback: string): string {
  const result = z.object({ message: z.string().min(1) }).safeParse(cause);
  if (result.success) return result.data.message;
  return typeof cause === "string" && cause.trim() ? cause : fallback;
}
