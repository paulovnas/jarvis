import { describe, expect, it } from "vitest";

import { publicationProposalSchema } from "./publication";

describe("publicationProposalSchema", () => {
  it("keeps older commit proposals compatible while defaulting new operations", () => {
    const proposal = publicationProposalSchema.parse({
      summary: "Publicar alteração",
      repositories: [{
        path: ".",
        files: ["src/App.tsx"],
        branch: null,
        commitMessage: "feat: update app",
        pullRequest: null,
      }],
    });

    expect(proposal.repositories[0]).toMatchObject({
      reset: null,
      push: "none",
      commitMessage: "feat: update app",
    });
    expect(proposal.authorization).toBeNull();
  });

  it("accepts an action-only soft reset without a commit", () => {
    const proposal = publicationProposalSchema.parse({
      summary: "Reabrir o último commit",
      repositories: [{
        path: "backend",
        reset: { mode: "soft", target: "HEAD^" },
        files: [],
        branch: null,
        commitMessage: null,
        push: "none",
        pullRequest: null,
      }],
    });

    expect(proposal.repositories[0].reset).toEqual({ mode: "soft", target: "HEAD^" });
  });

  it("accepts verified current-request authorization metadata", () => {
    const proposal = publicationProposalSchema.parse({
      summary: "Publicar sem nova confirmação",
      authorization: {
        mode: "autonomous",
        evidence: "Faça commit e push sem me perguntar novamente.",
      },
      repositories: [{
        path: ".",
        files: ["src/App.tsx"],
        branch: null,
        commitMessage: "fix: update app",
        push: "normal",
        pullRequest: null,
      }],
    });

    expect(proposal.authorization).toEqual({
      mode: "autonomous",
      evidence: "Faça commit e push sem me perguntar novamente.",
    });
  });
});
