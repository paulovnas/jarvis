import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";
import { QuestionCard } from "@/components/chat/QuestionCard";
import { ToolApproval } from "@/components/chat/ToolApproval";
import type { PendingAuthoring } from "@/core/authoring";
import type { QuestionDraft } from "@/core/questions";
import type { ValidationBatch, ValidationItem } from "@/core/workflow";
import type { Mutation, RemoteChat } from "./client";
import { cn } from "@/lib/utils";

export type RemoteAction = (method: Mutation, params: Record<string, unknown>) => Promise<boolean>;

function ReviewProposal({ request, agentId, conversationId, onAction, busy, formDrafts, focused }: {
  request: PendingAuthoring; agentId: string; conversationId: string; onAction: RemoteAction; busy: boolean; formDrafts: Map<string, string>; focused: boolean;
}) {
  const draftKey = JSON.stringify([conversationId, agentId, request.turnId, request.toolId]);
  const [note, setNote] = useState(() => formDrafts.get(draftKey) ?? "");
  const publication = request.target.kind === "publication";
  const decision = (approved: boolean) => onAction(publication ? "validation" : "authoring", {
    conversationId, agentId, ...(publication ? { kind: "publication" } : {}),
    decision: { turnId: request.turnId, toolId: request.toolId, approved, note: note.trim() || null },
  });
  return <Card role="region" aria-label={publication ? "Aprovação Git e GitHub" : "Aprovação de configuração"} className={cn("remote-review-panel", focused && "remote-review-focused")}>
    <CardHeader className="remote-review-header"><CardTitle>{publication ? "Revisar Git e GitHub" : "Revisar configuração"}</CardTitle></CardHeader>
    <CardContent className="remote-review-content flex min-w-0 flex-col gap-4">
      <CardDescription className="whitespace-pre-wrap break-words">{request.summary}</CardDescription>
      {request.target.kind === "publication" ? request.target.after.repositories.map(repository => <Card key={repository.path} size="sm">
        <CardHeader><CardTitle className="break-all font-mono text-xs">{repository.path === "." ? "Raiz do projeto" : repository.path}</CardTitle></CardHeader>
        <CardContent className="flex flex-col gap-3">
          {repository.reset && <Alert><AlertTitle>Reorganizar histórico</AlertTitle><AlertDescription><code className="break-all font-mono">git reset --soft {repository.reset.target}</code></AlertDescription></Alert>}
          {repository.branch && <p className="break-all">Branch: <code className="font-mono">{repository.branch}</code></p>}
          {repository.commitMessage && <section><p className="micro-label">Commit</p><pre className="remote-code">{repository.commitMessage}</pre></section>}
          {repository.files.length > 0 && <Collapsible defaultOpen={!focused}>
            <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-9 w-full cursor-pointer justify-start px-0 text-xs">Arquivos · {repository.files.length}<ChevronDown data-icon="inline-end" className="ml-auto shrink-0 transition-transform group-data-panel-open:rotate-180" /></CollapsibleTrigger>
            <CollapsibleContent><ul className="flex flex-col gap-1 pt-1">{repository.files.map(file => <li key={file} className="break-all font-mono text-xs">{file}</li>)}</ul></CollapsibleContent>
          </Collapsible>}
          {repository.sync !== "none" && <p className="break-words">Sincronização: <code className="font-mono">{repository.sync}</code>{repository.syncBase && <> · <code className="break-all font-mono">{repository.syncBase}</code></>}</p>}
          {repository.push !== "none" && <Alert><AlertTitle>{repository.push === "force_with_lease" ? "Push com force-with-lease" : "Push para origin"}</AlertTitle><AlertDescription>{repository.push === "force_with_lease" ? "Reescreve a branch remota se ela não avançou desde a última leitura." : "Publica o HEAD e configura o upstream."}</AlertDescription></Alert>}
          {repository.pullRequest && <section className="flex flex-col gap-2">
            <p className="micro-label">Pull request {repository.pullRequest.draft ? "· Rascunho" : ""}</p>
            <p className="break-all font-mono text-xs">Base: {repository.pullRequest.base}</p><p className="break-words font-medium">{repository.pullRequest.title}</p>
            <Collapsible defaultOpen={!focused}>
              <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-9 w-full cursor-pointer justify-start px-0 text-xs">Descrição do PR<ChevronDown data-icon="inline-end" className="ml-auto shrink-0 transition-transform group-data-panel-open:rotate-180" /></CollapsibleTrigger>
              <CollapsibleContent><p className="whitespace-pre-wrap break-words pt-1">{repository.pullRequest.body}</p></CollapsibleContent>
            </Collapsible>
            {repository.pullRequest.merge && <Alert><AlertTitle>Merge após criar ou localizar</AlertTitle><AlertDescription>{repository.pullRequest.merge.method}{repository.pullRequest.merge.deleteBranch ? " · Excluir branch após o merge" : ""}</AlertDescription></Alert>}
          </section>}
        </CardContent>
      </Card>) : <section><p className="micro-label">Proposta completa</p><pre className="remote-code">{JSON.stringify(request.target, null, 2)}</pre></section>}
      <FieldGroup><Field><FieldLabel htmlFor={`note-${agentId}-${request.toolId}`}>Orientação para o agente (opcional)</FieldLabel><Textarea id={`note-${agentId}-${request.toolId}`} maxLength={2000} disabled={busy} value={note} onChange={event => { formDrafts.set(draftKey, event.target.value); setNote(event.target.value); }} /></Field></FieldGroup>
      {publication && note.trim() && <Alert><AlertTitle>Solicitar revisão</AlertTitle><AlertDescription>O agente incorporará sua orientação e apresentará outra proposta antes de executar as ações.</AlertDescription></Alert>}
    </CardContent>
    <CardFooter className="remote-review-actions flex-wrap gap-2"><Button variant="outline" className="min-w-0 cursor-pointer" disabled={busy} onClick={() => { void decision(false); }}>Recusar</Button><Button className="min-w-0 cursor-pointer" disabled={busy} onClick={() => { void decision(true); }}>{publication ? note.trim() ? "Enviar para revisão" : "Aprovar e executar" : "Aprovar e salvar"}</Button></CardFooter>
  </Card>;
}

function ValidationItemForm({ item, batch, conversationId, busy, onAction, formDrafts }: {
  item: ValidationItem; batch: ValidationBatch; conversationId: string; busy: boolean; onAction: RemoteAction; formDrafts: Map<string, string>;
}) {
  const draftKey = JSON.stringify([conversationId, batch.id, item.id]);
  const [reason, setReason] = useState(() => formDrafts.get(draftKey) ?? item.reason ?? "");
  const disabled = busy || batch.submitted || batch.stale;
  const decide = (decision: "approved" | "rejected") => onAction("validation", {
    conversationId, kind: "item", batchId: batch.id, itemId: item.id, decision,
    ...(decision === "rejected" ? { reason: reason.trim() } : {}),
  });
  return <Card size="sm"><CardHeader><CardTitle className="break-words">{item.title}</CardTitle><CardDescription>{item.decision === "approved" ? "Aprovado" : item.decision === "rejected" ? "Reprovado" : "Pendente"}</CardDescription></CardHeader>
    <CardContent className="flex flex-col gap-3"><ol className="flex list-decimal flex-col gap-2 pl-5">{item.steps.map((step, index) => <li key={index} className="whitespace-pre-wrap break-words">{step}</li>)}</ol>
      <Alert><AlertTitle>Resultado esperado</AlertTitle><AlertDescription className="whitespace-pre-wrap break-words">{item.expected}</AlertDescription></Alert>
      <FieldGroup><Field><FieldLabel htmlFor={`reason-${item.id}`}>O que não funcionou? (para reprovar)</FieldLabel><Textarea id={`reason-${item.id}`} maxLength={4000} value={reason} disabled={disabled} onChange={event => { formDrafts.set(draftKey, event.target.value); setReason(event.target.value); }} /></Field></FieldGroup>
    </CardContent><CardFooter className="flex-wrap gap-2"><Button variant="destructive" className="cursor-pointer" disabled={disabled || !reason.trim()} onClick={() => { void decide("rejected"); }}>Reprovar item</Button><Button variant="outline" className="cursor-pointer" disabled={disabled} onClick={() => { void decide("approved"); }}>Aprovar item</Button></CardFooter>
  </Card>;
}

export function PendingForms({ bundle, projectPath, busy, onAction, questionDrafts, formDrafts: providedFormDrafts, focused = false }: { bundle: RemoteChat; projectPath: string; busy: boolean; onAction: RemoteAction; questionDrafts?: Map<string, QuestionDraft>; formDrafts?: Map<string, string>; focused?: boolean }) {
  const [localDrafts] = useState(() => new Map<string, QuestionDraft>());
  const [localFormDrafts] = useState(() => new Map<string, string>());
  const drafts = questionDrafts ?? localDrafts;
  const formDrafts = providedFormDrafts ?? localFormDrafts;
  const chat = bundle.chat;
  const owners = [{ id: "main", title: "Jarvis", activeTurnId: chat.activeTurnId, pendingQuestion: chat.pendingQuestion, pendingApproval: chat.pendingApproval, pendingAuthoring: chat.pendingAuthoring }, ...(bundle.workflow?.agents.filter(agent => agent.id !== "main") ?? [])];
  const validation = bundle.workflow?.validation;
  return <FieldSet disabled={busy} className={cn("remote-pending flex min-w-0 flex-col gap-3", focused && "remote-pending-focused")}>
    {owners.filter(owner => owner.pendingQuestion || owner.pendingApproval || owner.pendingAuthoring).map(owner => <section key={owner.id} className="remote-decision-owner flex min-w-0 flex-col gap-2">
      <Badge variant="outline" className="self-start">Precisa de você · {owner.title}</Badge>
      {owner.pendingQuestion && <div className="remote-question"><QuestionCard key={`${chat.conversationId}-${owner.id}-${owner.pendingQuestion.turnId}-${owner.pendingQuestion.toolId}`} request={owner.pendingQuestion} drafts={drafts} draftKey={JSON.stringify([chat.conversationId, owner.id, owner.pendingQuestion.turnId, owner.pendingQuestion.toolId])} autoFocus={false}
        onAnswer={(request, response) => onAction("question", { conversationId: chat.conversationId, agentId: owner.id, turnId: request.turnId, toolId: request.toolId, response })}
        onInteract={request => onAction("question", { conversationId: chat.conversationId, agentId: owner.id, turnId: request.turnId, toolId: request.toolId, pause: true })} /></div>}
      {owner.pendingApproval && owner.activeTurnId && <ToolApproval key={`${owner.activeTurnId}-${owner.pendingApproval.tool.id}`} request={owner.pendingApproval} projectPath={projectPath} onAnswer={decision => onAction("approval", { conversationId: chat.conversationId, agentId: owner.id, turnId: owner.activeTurnId, toolId: owner.pendingApproval?.tool.id, decision })} />}
      {owner.pendingAuthoring && <ReviewProposal key={`${chat.conversationId}-${owner.id}-${owner.pendingAuthoring.toolId}`} request={owner.pendingAuthoring} agentId={owner.id} conversationId={chat.conversationId} onAction={onAction} busy={busy} formDrafts={formDrafts} focused={focused} />}
    </section>)}
    {validation && <section className="flex min-w-0 flex-col gap-3" aria-label="Validação manual"><Separator /><p className="micro-label">Validação manual</p>
      {validation.stale && <Alert><AlertTitle>Aguardando nova rodada</AlertTitle><AlertDescription>Esta validação já não corresponde à execução atual.</AlertDescription></Alert>}
      {validation.items.map(item => <ValidationItemForm key={`${validation.id}-${item.id}`} item={item} batch={validation} conversationId={chat.conversationId} busy={busy} onAction={onAction} formDrafts={formDrafts} />)}
      <Button className="cursor-pointer" disabled={busy || validation.submitted || validation.stale || !validation.items.length || validation.items.some(item => item.decision === "pending")} onClick={() => { void onAction("validation", { conversationId: chat.conversationId, kind: "submit", batchId: validation.id }); }}>{validation.submitted ? "Resultado encaminhado" : "Encaminhar resultado"}</Button>
    </section>}
  </FieldSet>;
}
