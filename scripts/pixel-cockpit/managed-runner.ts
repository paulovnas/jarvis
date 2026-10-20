interface RunnerStatus {
  jobId: string;
  state: string;
  processId?: number;
  resumeRequired?: boolean;
  cancellationPending?: boolean;
}

/** An inaccessible process is uncertain, so it must never be duplicated. */
export function processIsAlive(id?: number) {
  if (!id || !Number.isInteger(id) || id < 1) return false;
  try { process.kill(id, 0); return true; }
  catch (error) { return (error as NodeJS.ErrnoException).code !== "ESRCH"; }
}

export function interruptedRunnerStatus<T extends RunnerStatus>(status: T, alive = processIsAlive) {
  if (alive(status.processId)) return status;
  return {
    ...status, state: "failed" as const, resumeRequired: true,
    message: "O executor foi interrompido. Os arquivos foram preservados e nenhuma geração foi reiniciada.",
    diagnostic: {
      kind: "runner_failed" as const,
      title: "Execução interrompida",
      userMessage: "O processo do Codex terminou antes de confirmar a entrega. É necessário recuperar este mesmo trabalho.",
      suggestion: "Consulte os arquivos e use resumeInterruptedCodexJob com confirmação explícita. Para finalizar uma imagem existente, use process_existing.",
    },
  };
}

export function assertExplicitResume(status: RunnerStatus, confirmed: boolean, tracked: boolean, alive = processIsAlive) {
  if (!confirmed) throw new Error("A retomada exige confirm=true após autorização explícita.");
  if (tracked || alive(status.processId)) throw new Error("Este executor ainda está em andamento; a retomada não foi iniciada.");
  if (status.cancellationPending || !(status.state === "running" || status.resumeRequired)) {
    throw new Error("Somente um trabalho interrompido pode ser retomado. Consulte seu resultado antes de executar outra ação.");
  }
}

export function cancellationMessage(reason: string | undefined, confirmed: boolean) {
  const origin = reason === "user" ? "por solicitação do usuário" : "pelo controle do torneio";
  return confirmed ? `Execução cancelada ${origin}.` : `Cancelamento solicitado ${origin}; aguardando o encerramento do executor.`;
}

/** Preserve upstream normalization; make process restart an explicit managed action. */
export function adoptManagedRunner(source: string) {
  const marker = "    return resumeUntrackedCodexRunner(status);";
  if (!source.includes(marker)) throw new Error("Original runner reconciliation changed; review adoption.");
  source = 'import { interruptedRunnerStatus, assertExplicitResume, cancellationMessage } from "../managed-runner";\n' + source;
  source = source.replace("type CodexRunnerStatus = {", "type CodexRunnerStatus = {\n  cancelled?: boolean;\n  cancellationReason?: string;\n  resumeRequired?: boolean;");
  source = source.replace(marker, `    const interrupted = interruptedRunnerStatus(status);
    if (interrupted !== status) await writeRunnerStatus(interrupted);
    return interrupted;`);
  source = source.replace("  if (!shouldBuildDiagnostic(status)) return status;",
    '  if ("cancelled" in status && status.cancelled) { const clean = {...status}; delete clean.diagnostic; return clean; }\n  if ("resumeRequired" in status && status.resumeRequired) return status;\n  if (!shouldBuildDiagnostic(status)) return status;');
  source = source.replace("async function cancelCodexRunner(jobId: string) {", 'async function cancelCodexRunner(jobId: string, reason = "tournament") {');
  source = source.replace("sendJson(response, 200, await cancelCodexRunner(jobId));", 'sendJson(response, 200, await cancelCodexRunner(jobId, "user"));');
  source = source.replace("  cancellingRunnerJobIds.add(jobId);", "  cancellingRunnerJobIds.add(jobId);\n  managedCancellationReasons.set(jobId, reason);");
  source = source.replace('message: "Codex runner cancelled after an animation tournament winner was chosen.",',
    "message: cancellationMessage(reason, false),\n    cancelled: true,\n    cancellationReason: reason,");
  source = source.replace('message: wasCancelled ? "Codex runner cancelled after an animation tournament winner was chosen."',
    'cancelled: wasCancelled || undefined,\n        cancellationReason: wasCancelled ? managedCancellationReasons.get(job.id) : undefined,\n        message: wasCancelled ? cancellationMessage(managedCancellationReasons.get(job.id), true)');
  source = source.replace("    const jobStatusMatch =", `
    const jobResumeMatch = pathname.match(/^\\/api\\/codex\\/jobs\\/([^/]+)\\/resume$/);
    if (request.method === "POST" && jobResumeMatch) {
      const jobId = decodeURIComponent(jobResumeMatch[1]);
      if (!isSafeJobId(jobId)) { sendJson(response, 400, { error: "Identificador de trabalho inválido." }); return; }
      const body = await readJson(request) as { confirm?: boolean; mode?: string };
      const status = await getRunnerStatus(jobId);
      assertExplicitResume(status, body.confirm === true, runnerProcesses.has(jobId) || resumingRunnerJobs.has(jobId));
      if (body.mode !== "process_existing" && body.mode !== "resume") {
        sendJson(response, 400, {error:"Escolha process_existing para finalizar artefatos existentes ou resume para retomar a tarefa original."}); return;
      }
      // Scope this directive to the explicit resumed invocation; the original job/prompt stays intact.
      managedRecoveryModes.set(jobId, body.mode);
      try { sendJson(response, 200, {status: await resumeUntrackedCodexRunner(status)}); }
      finally { managedRecoveryModes.delete(jobId); }
      return;
    }
    const jobStatusMatch =`);
  source = source.replace("function buildCodexRunnerPrompt(", 'const managedCancellationReasons = new Map<string, string>();\nconst managedRecoveryModes = new Map<string, string>();\nfunction buildCodexRunnerPrompt(');
  source = source.replace('    ...(resumed ? [', `    ...(managedRecoveryModes.get(job.id) === "process_existing" ? [
      "MANAGED RECOVERY: process the existing image artifacts only. Do not call imagegen or any image-generation tool, API, or provider.",
      "Use the original raw image in outbox/.staging. Preserve identity and finalize normalization, validation, manifests, and exports for THIS SAME job.",
      "If an essential source is absent, report that precise dependency and stop without generating another image."
    ] : []),
    ...(resumed ? [`);
  return source;
}
