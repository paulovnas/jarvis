import { describe, expect, it } from "vitest";
import { executionBaseline, observeExecution, shouldTrackManualGeneration } from "./execution-state";

describe("Pixel Cockpit execution receipts", () => {
  it("publishes a manual job once and does not duplicate worker-owned generation", () => {
    const baseline = executionBaseline({});
    const running = { codexJobs: [{ id: "manual-job" }] };
    expect(shouldTrackManualGeneration(running, baseline, false, false)).toBe(true);
    expect(shouldTrackManualGeneration(running, baseline, false, true)).toBe(false);
    expect(shouldTrackManualGeneration(running, baseline, true, false)).toBe(false);
    expect(shouldTrackManualGeneration(running, executionBaseline(running), false, false)).toBe(false);
  });
  it("does not treat rebuilding a saved timeline preview as a new generation", () => {
    const result = observeExecution({ isAnimationPreviewBuilding: true }, executionBaseline({}));
    expect(result.pending).toBe(true);
    expect(result.generationQueued).toBe(false);
  });
  it("keeps queued batch work running before Codex creates the first job", () => {
    const baseline = executionBaseline({});
    expect(observeExecution({
      codexJobs: [],
      animationTournamentMonitors: [{ manifest: { tournamentId: "batch-cell", state: "queued" } }],
    }, baseline).pending).toBe(true);
  });

  it("reports the real motion-pilot review state without completing generation", () => {
    const result = observeExecution({
      animationTournamentMonitors: [{ manifest: { tournamentId: "pilot", state: "pilot-review" } }],
    }, executionBaseline({}));
    expect(result.pending).toBe(false);
    expect(result.review?.tournamentId).toBe("pilot");
  });

  it("recognizes a generated direction repair awaiting acceptance within an already accepted tournament", () => {
    const accepted = {tournamentId:"accepted",state:"accepted",candidates:[{jobId:"winner",state:"accepted"}]};
    const candidate = {jobId:"front-repair",state:"quality-evaluated",repairDirections:["front"],score:3400,warningCount:0};
    const manifest = {...accepted,candidates:[...accepted.candidates,candidate]};
    const baseline = executionBaseline({animationTournamentMonitors:[{manifest:accepted}]});
    const result = observeExecution({animationTournamentMonitors:[{manifest}]},baseline);
    expect(result.pending).toBe(false);
    expect(result.repairReview).toEqual({manifest,candidate});
    expect(manifest.state).toBe("accepted");
    expect(candidate.state).toBe("quality-evaluated");
    expect(observeExecution({animationTournamentMonitors:[{manifest}]},
      executionBaseline({animationTournamentMonitors:[{manifest}]})).repairReview).toBeUndefined();
    expect(observeExecution({animationTournamentMonitors:[{manifest:{...manifest,updatedAt:"later"}}]},
      executionBaseline({animationTournamentMonitors:[{manifest}]})).repairReview).toBeUndefined();
  });

  it("does not block a new image because an older tournament awaits review", () => {
    const state = {
      animationTournamentMonitors: [{ manifest: { tournamentId: "old", state: "pilot-review" } }],
      codexFailureNotices: [{ jobId: "older-failure" }],
      history: [{ id: "before" }],
    };
    const result = observeExecution({ ...state, history: [...state.history, { id: "new-image" }] }, executionBaseline(state));
    expect(result.review).toBeUndefined();
    expect(result.failures).toEqual([]);
    expect(result.fresh.map((entry) => entry.id)).toEqual(["new-image"]);
  });

  it("preserves completed files alongside a failed candidate instead of claiming the batch succeeded", () => {
    const result = observeExecution({
      history: [{ id: "confirmed-image" }],
      codexFailureNotices: [{ jobId: "failed-cell", message: "generation failed" }],
      animationTournamentMonitors: [{ manifest: { tournamentId: "failed-cell", state: "failed" } }],
    }, executionBaseline({}));
    expect(result.pending).toBe(false);
    expect(result.fresh).toHaveLength(1);
    expect(result.failures).toHaveLength(1);
    expect(result.review?.state).toBe("failed");
  });

  it("does not wait forever on a stale running tournament whose candidates have all stopped", () => {
    const stale = {tournamentId:"batch-idle",state:"running",candidates:[{
      jobId:"idle-job",state:"failed",reason:"Direction split QA failed: idle breathing motion too static across directions (0/1 readable rows, 1.1% average frame change)",
    }]};
    const result = observeExecution({
      codexJobs:[],history:[{id:"confirmed-walk"}],
      animationTournamentMonitors:[{manifest:stale}],
    },executionBaseline({}));
    expect(result.pending).toBe(false);
    expect(result.stoppedTournaments).toEqual([{manifest:stale,outcome:"failed"}]);
    expect(result.fresh).toEqual([{id:"confirmed-walk"}]);
    expect(stale.state).toBe("running");
  });

  it("keeps another live batch cell pending and treats unpublished evaluated candidates as review", () => {
    const state = {
      codexJobs:[{id:"live-job"}],
      animationTournamentMonitors:[
        {manifest:{tournamentId:"failed",state:"running",candidates:[{jobId:"done",state:"failed"}]}},
        {manifest:{tournamentId:"live",state:"running",candidates:[{jobId:"live-job",state:"running"}]}},
      ],
    };
    expect(observeExecution(state,executionBaseline({})).pending).toBe(true);
    const reviewed = observeExecution({animationTournamentMonitors:[{manifest:{
      tournamentId:"review",state:"running",candidates:[{jobId:"evaluated",state:"quality-evaluated"}],
    }}]},executionBaseline({}));
    expect(reviewed.pending).toBe(false);
    expect(reviewed.stoppedTournaments[0].outcome).toBe("review");
  });
});
