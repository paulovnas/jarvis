type Entry = Record<string, unknown>;
const entries = (value: unknown): Entry[] => Array.isArray(value) ? value as Entry[] : [];
const record = (value: unknown): Entry => value && typeof value === "object" ? value as Entry : {};

export interface ExecutionBaseline {
  history: Set<unknown>;
  failures: Set<unknown>;
  tournaments: Map<unknown, string>;
  jobs: Set<unknown>;
}

export function executionBaseline(state: Entry): ExecutionBaseline {
  return {
    history: new Set(entries(state.history).map((entry) => entry.id)),
    failures: new Set(entries(state.codexFailureNotices).map((entry) => entry.jobId)),
    jobs: new Set(entries(state.codexJobs).map((entry) => entry.id)),
    tournaments: new Map(entries(state.animationTournamentMonitors).map((entry) => {
      const manifest = record(entry.manifest);
      return [manifest.tournamentId, JSON.stringify(manifest)];
    })),
  };
}

export function shouldTrackManualGeneration(state: Entry, baseline: ExecutionBaseline, workerActive: boolean, manualActive: boolean) {
  if (workerActive || manualActive) return false;
  const observed = observeExecution(state, baseline);
  return entries(state.codexJobs).some((entry) => !baseline.jobs.has(entry.id))
    || observed.tournaments.some((manifest) => ["queued", "running", "pilot-expanding"].includes(String(manifest.state))
      && !observed.stoppedTournaments.some((stopped) => stopped.manifest === manifest));
}

/** Derive presentation from confirmed candidate outcomes; never rewrite the original manifest. */
export function stoppedTournamentOutcome(value: unknown, jobs: unknown): "failed" | "review" | "cancelled" | undefined {
  const manifest = record(value);
  if (!["running", "pilot-expanding"].includes(String(manifest.state))) return;
  const started = entries(manifest.candidates).filter((candidate) => candidate.jobId);
  if (!started.length || entries(jobs).some((job) =>
    job.tournamentId === manifest.tournamentId || started.some((candidate) => candidate.jobId === job.id))) return;
  if (started.some((candidate) => !["failed", "cancelled", "quality-evaluated", "accepted"].includes(String(candidate.state)))) return;
  if (started.some((candidate) => ["quality-evaluated", "accepted"].includes(String(candidate.state)))) return "review";
  return started.some((candidate) => candidate.state === "failed") ? "failed" : "cancelled";
}

/** Observe only work started or changed by this command; older reviews are unrelated. */
export function observeExecution(state: Entry, baseline: ExecutionBaseline) {
  const tournaments = entries(state.animationTournamentMonitors).map((entry) => record(entry.manifest))
    .filter((manifest) => baseline.tournaments.get(manifest.tournamentId) !== JSON.stringify(manifest));
  const stoppedTournaments = tournaments.flatMap((manifest) => {
    const outcome = stoppedTournamentOutcome(manifest, state.codexJobs);
    return outcome ? [{manifest,outcome}] : [];
  });
  const generationQueued = entries(state.codexJobs).length > 0
    || tournaments.some((manifest) => ["queued", "running", "pilot-expanding"].includes(String(manifest.state))
      && !stoppedTournaments.some((stopped) => stopped.manifest === manifest));
  const pending = generationQueued || Boolean(state.isBusy)
    || Boolean(state.isAnimationPreviewBuilding) || Boolean(state.isEffectPreviewBuilding);
  const failures = entries(state.codexFailureNotices).filter((entry) => !baseline.failures.has(entry.jobId));
  const review = tournaments.find((manifest) => ["review", "pilot-review", "failed"].includes(String(manifest.state)));
  const repairReview = tournaments.flatMap((manifest) => {
    if (manifest.state !== "accepted") return [];
    const previous = record(JSON.parse(baseline.tournaments.get(manifest.tournamentId) ?? "{}"));
    const candidate = entries(manifest.candidates).find((item) => item.jobId && item.state === "quality-evaluated"
      && Array.isArray(item.repairDirections) && item.repairDirections.length > 0
      && !entries(previous.candidates).some((old) => old.jobId === item.jobId && old.state === "quality-evaluated"));
    return candidate ? [{manifest,candidate}] : [];
  })[0];
  return {
    pending,
    generationQueued,
    tournaments,
    failures,
    review,
    repairReview,
    stoppedTournaments,
    fresh: entries(state.history).filter((entry) => !baseline.history.has(entry.id)),
  };
}
