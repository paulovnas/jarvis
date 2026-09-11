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
    files: z.array(z.string()),
    branch: z.string().nullable(),
    commitMessage: z.string(),
    pullRequest: pullRequestProposalSchema.nullable(),
  })),
});

export type PublicationProposal = z.infer<typeof publicationProposalSchema>;

export const PULL_REQUEST_MODE_LABELS: Record<PullRequestMode, string> = {
  disabled: "Não perguntar",
  ask_pr: "Perguntar sobre PR",
  ask_pr_merge: "Perguntar sobre PR e merge",
};
