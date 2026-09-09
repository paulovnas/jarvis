import { z } from "zod";
import { customAgentSchema, customFlowSchema } from "./workflow-catalog";

const targetSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("agent"), before: customAgentSchema.nullable(), after: customAgentSchema }),
  z.object({ kind: z.literal("flow"), before: customFlowSchema.nullable(), after: customFlowSchema }),
]);

export const pendingAuthoringSchema = z.object({
  turnId: z.string(),
  toolId: z.string(),
  action: z.enum(["create", "update"]),
  summary: z.string(),
  catalogRevision: z.number().int().nonnegative(),
  target: targetSchema,
  agentReferences: z.array(z.object({ id: z.string(), name: z.string() })),
});

export type PendingAuthoring = z.infer<typeof pendingAuthoringSchema>;
