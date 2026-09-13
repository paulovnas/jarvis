import { z } from "zod";

export const pullRequestModeSchema = z.enum(["disabled", "ask_pr", "ask_pr_merge"]);
export type PullRequestMode = z.infer<typeof pullRequestModeSchema>;

export const publicationSettingsSchema = z.object({
  projectId: z.string().min(1),
  publishPrompt: z.string().min(1).max(16_000),
  prMode: pullRequestModeSchema,
  prPrompt: z.string().min(1).max(16_000),
  ghAvailable: z.boolean(),
});

export type PublicationSettings = z.infer<typeof publicationSettingsSchema>;

const mergeProposalSchema = z.object({
  method: z.enum(["merge", "squash", "rebase"]),
  deleteBranch: z.boolean(),
});

const resetProposalSchema = z.object({
  mode: z.literal("soft"),
  target: z.string(),
});

const pullRequestProposalSchema = z.object({
  base: z.string(),
  title: z.string(),
  body: z.string(),
  draft: z.boolean(),
  merge: mergeProposalSchema.nullable(),
});

export const publicationProposalSchema = z.object({
  summary: z.string(),
  repositories: z.array(z.object({
    path: z.string(),
    reset: resetProposalSchema.nullable().default(null),
    files: z.array(z.string()).default([]),
    branch: z.string().nullable().default(null),
    commitMessage: z.string().nullable().default(null),
    push: z.enum(["none", "normal", "force_with_lease"]).default("none"),
    pullRequest: pullRequestProposalSchema.nullable().default(null),
  })),
});

export type PublicationProposal = z.infer<typeof publicationProposalSchema>;

export const PULL_REQUEST_MODE_LABELS: Record<PullRequestMode, string> = {
  disabled: "Não perguntar",
  ask_pr: "Perguntar sobre PR",
  ask_pr_merge: "Perguntar sobre PR e merge",
};
