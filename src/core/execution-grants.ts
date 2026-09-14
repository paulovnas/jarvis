import { z } from "zod";
import type { ExecutionGrantSummary as GeneratedExecutionGrantSummary } from "@/generated/ipc";
import { executionEffectsSchema } from "./chat";

export const executionGrantSummarySchema = z.object({
  id: z.string(),
  scope: z.enum(["conversation", "project", "repository"]),
  scopeRoot: z.string().nullable(),
  matchKind: z.enum(["exact", "commandPrefix"]),
  duration: z.enum(["once", "session", "until", "persistent"]),
  expiresAt: z.number().nonnegative().nullable(),
  subject: z.string(),
  effects: executionEffectsSchema,
  createdAt: z.number().nonnegative(),
  lastUsedAt: z.number().nonnegative().nullable(),
  uses: z.number().int().nonnegative(),
});

export const executionGrantListSchema = z.array(executionGrantSummarySchema);
export type ExecutionGrantSummary = GeneratedExecutionGrantSummary;
