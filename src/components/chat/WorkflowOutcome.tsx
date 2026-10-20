import type { WorkflowResult } from "@/core/workflow";

export function WorkflowOutcome({ result, compact = false }: { result: WorkflowResult; compact?: boolean }) {
  const { handoff, status, error } = result;
  const objective = handoff?.objective;
  const blocked = status === "blocked";
  const failed = status === "failed";
  const pending = objective && objective.status !== "completed";
  if (!blocked && !failed && !pending) return null;
  const title = failed ? "Falha de execução" : blocked ? handoff?.verdict === "rework" ? "Correções necessárias" : "Motivo do bloqueio" : "Pedido com pendências";
  const reason = (failed ? error : objective?.reason) || handoff?.summary || error || "O agente não registrou a causa. Consulte o histórico antes de retomar.";
  return <span role="group" aria-label={title} className="mt-2 block space-y-1 break-words text-left text-[11px] leading-5 text-muted-foreground">
    <span className={`block font-medium ${blocked || failed ? "text-destructive" : "text-foreground"}`}>{title}</span>
    <span className="block">{reason}</span>
    {!compact && blocked && !objective && handoff?.limitations?.map((item, index) => <span key={index} className="block">· {item}</span>)}
    {!compact && objective?.remaining.length ? <span className="block"><span className="font-medium">Pendente:</span>{objective.remaining.map((item, index) => <span key={index} className="block">· {item}</span>)}</span> : null}
    {!compact && (blocked || failed) && <span className="block"><span className="font-medium">Tentativas de recuperação:</span>{objective?.recoveryAttempts.length ? objective.recoveryAttempts.map((item, index) => <span key={index} className="block">· {item}</span>) : <span className="block">Nenhuma tentativa registrada.</span>}</span>}
    {(blocked || failed || pending) && <span className="block"><span className="font-medium">Próximo passo: </span>{objective?.nextAction || "Consultar o histórico do agente para identificar o pré-requisito e retomar somente o trabalho pendente."}</span>}
  </span>;
}
