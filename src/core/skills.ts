import { z } from "zod";

export const skillSchema = z.object({
  id: z.string(), name: z.string(), description: z.string(), origin: z.enum(["jarvis", "agents", "project"]), path: z.string(),
  enabled: z.boolean(), automatic: z.boolean(), source: z.string().nullable(), marketplaceId: z.string().nullable(),
  removalPath: z.string().optional(), linked: z.boolean().optional(),
  managed: z.boolean().optional(),
  updateAvailable: z.boolean(), updateError: z.string().nullable(),
});
export const skillsSnapshotSchema = z.object({ includeAgents: z.boolean(), directory: z.string(), skills: z.array(skillSchema), warnings: z.array(z.string()) });
export const skillDetailSchema = z.object({ name: z.string(), description: z.string(), content: z.string(), path: z.string().nullable(), source: z.string().nullable(), files: z.array(z.string()) });
export const marketplaceSchema = z.array(z.object({ id: z.string(), skillId: z.string(), name: z.string(), source: z.string(), installs: z.number().nonnegative() }));
export const skillUpdateSchema = z.object({ snapshot: skillsSnapshotSchema, updated: z.number().int().nonnegative(), errors: z.array(z.string()) });
export type Skill = z.infer<typeof skillSchema>;
export type SkillSnapshot = z.infer<typeof skillsSnapshotSchema>;
export type SkillDetail = z.infer<typeof skillDetailSchema>;
export type MarketplaceSkill = z.infer<typeof marketplaceSchema>[number];
export function skillError(cause: unknown): string {
  if (typeof cause === "object" && cause !== null && "code" in cause && cause.code === "skill_error" && "message" in cause && typeof cause.message === "string") return cause.message;
  return "Não foi possível concluir a operação da skill.";
}
