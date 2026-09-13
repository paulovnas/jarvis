import { useMemo, useState } from "react";
import { Bot, FileCode2, FolderGit2, GitBranch, GitCommitHorizontal, GitCompareArrows, GitMerge, GitPullRequest, LockKeyhole, RotateCcw, Route, ShieldCheck, Sparkles, Upload, X } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { WorkflowIdentityIcon } from "@/components/agents/WorkflowIdentityIcon";
import { LazyChatMarkdown } from "./LazyChatMarkdown";
import { AGENT_USAGE_LABELS, CAPABILITY_LABELS, type CustomAgent, type CustomFlow } from "@/core/workflow-catalog";
import { agentAppearance, flowAppearance } from "@/core/workflow-appearance";
import type { PendingAuthoring } from "@/core/authoring";
import type { PublicationProposal } from "@/core/publication";
import { Hint } from "@/components/ui/hint";

type Decision = (approved: boolean, note: string | null) => Promise<boolean>;

function changedFields(request: PendingAuthoring): string[] {
  const target = request.target;
  if (target.kind === "publication") return [];
  if (target.kind === "agent") {
    if (!target.before) return [];
    const before = target.before;
    const after = target.after;
    const fields = ["name", "description", "instructions", "usage", "capability", "deniedTools", "model", "appearance"] as const;
    return fields.filter(field => JSON.stringify(before[field]) !== JSON.stringify(after[field]));
  }
  if (!target.before) return [];
  const before = target.before;
  const after = target.after;
  const fields = ["name", "description", "entry", "maxSteps", "steps", "appearance"] as const;
  return fields.filter(field => JSON.stringify(before[field]) !== JSON.stringify(after[field]));
}

const fieldLabels: Record<string, string> = {
  name: "Nome", description: "Descrição", instructions: "Instruções", usage: "Uso", capability: "Capacidade",
  deniedTools: "Permissões", model: "Modelo", appearance: "Aparência", entry: "Entrada",
  maxSteps: "Limite", steps: "Etapas",
};

function ModelSummary({ agent }: { agent: CustomAgent }) {
  return <div className="rounded-md border border-border bg-sidebar/70 p-3">
    <p className="micro-label mb-1.5 text-muted-foreground">Modelo</p>
    <p className="break-words font-mono text-xs text-foreground">{agent.model ? `${agent.model.account} / ${agent.model.model}${agent.model.reasoning ? ` / ${agent.model.reasoning}` : ""}` : "Herdar do chat"}</p>
  </div>;
}

function AgentReview({ agent }: { agent: CustomAgent }) {
  const denied = agent.deniedTools ?? [];
  return <div className="space-y-4">
    <Card className="gap-0 overflow-hidden border-border bg-card/80 p-0">
      <CardHeader className="flex flex-row items-start gap-3 border-b border-border bg-sidebar/45 p-4">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-md border border-border bg-background shadow-[inset_0_1px_0_#ffffff0d]">
          <WorkflowIdentityIcon appearance={agent.appearance} fallback={agentAppearance} className="size-5" />
        </div>
        <div className="min-w-0 flex-1"><CardTitle className="break-words text-base">{agent.name}</CardTitle><p className="mt-1 text-xs leading-5 text-muted-foreground">{agent.description || "Sem descrição."}</p></div>
        <div className="flex shrink-0 flex-wrap justify-end gap-1.5"><Badge variant="outline" className="text-[10px] text-primary">{AGENT_USAGE_LABELS[agent.usage]}</Badge><Badge variant="outline" className="text-[10px] text-muted-foreground">{CAPABILITY_LABELS[agent.capability]}</Badge></div>
      </CardHeader>
      <CardContent className="grid gap-3 p-4 sm:grid-cols-2">
        <ModelSummary agent={agent} />
        <div className="rounded-md border border-border bg-sidebar/70 p-3">
          <p className="micro-label mb-1.5 text-muted-foreground">Ferramentas</p>
          <p className="text-xs text-foreground">{denied.length ? `${denied.length} ${denied.length === 1 ? "restrição configurada" : "restrições configuradas"}` : "Todas as ferramentas compatíveis com a capacidade"}</p>
        </div>
      </CardContent>
    </Card>
    {denied.length > 0 && <section><p className="micro-label mb-2 text-muted-foreground">Ferramentas bloqueadas</p><div className="flex flex-wrap gap-1.5">{denied.map(tool => <Badge key={tool} variant="outline" className="font-mono text-[10px] text-muted-foreground">{tool}</Badge>)}</div></section>}
    <section><p className="micro-label mb-2 text-muted-foreground">Instruções propostas</p><div className="bead-prose max-h-80 overflow-y-auto rounded-md border border-border bg-background p-4 text-sm"><LazyChatMarkdown content={agent.instructions} /></div></section>
  </div>;
}

function FlowReview({ flow, references }: { flow: CustomFlow; references: PendingAuthoring["agentReferences"] }) {
  const names = new Map(references.map(agent => [agent.id, agent.name]));
  const indexes = new Map(flow.steps.map((step, index) => [step.id, index + 1]));
  return <div className="space-y-4">
    <Card className="gap-0 overflow-hidden border-border bg-card/80 p-0">
      <CardHeader className="flex flex-row items-start gap-3 border-b border-border bg-sidebar/45 p-4">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-md border border-border bg-background shadow-[inset_0_1px_0_#ffffff0d]">
          <WorkflowIdentityIcon appearance={flow.appearance} fallback={flowAppearance} className="size-5" />
        </div>
        <div className="min-w-0 flex-1"><CardTitle className="break-words text-base">{flow.name}</CardTitle><p className="mt-1 text-xs leading-5 text-muted-foreground">{flow.description || "Sem descrição."}</p></div>
        <Badge variant="outline" className="shrink-0 font-mono text-[10px] text-primary">até {flow.maxSteps} execuções</Badge>
      </CardHeader>
      <CardContent className="space-y-2 p-4">
        {flow.steps.map((step, index) => <article key={step.id} className="rounded-md border border-border bg-sidebar/65 p-3">
          <div className="flex min-w-0 items-start gap-3">
            <Badge className="size-6 shrink-0 justify-center rounded-full p-0 font-mono text-[10px]" variant={step.id === flow.entry ? "default" : "outline"}>{index + 1}</Badge>
            <div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{names.get(step.agentId) ?? "Agente indisponível"}</p>{step.instructions && <p className="mt-1 whitespace-pre-wrap text-xs leading-5 text-muted-foreground">{step.instructions}</p>}</div>
            {step.id === flow.entry && <Badge variant="outline" className="shrink-0 text-[9px] text-onedark-green">Início</Badge>}
          </div>
          <div className="mt-3 flex flex-wrap gap-2 border-t border-border pt-2 font-mono text-[10px] text-muted-foreground">
            <span>Concluir → {step.next ? `etapa ${indexes.get(step.next) ?? "?"}` : "finalizar"}</span>
            <span className="text-muted-foreground/40">•</span>
            <span>Correção → {step.onRework ? `etapa ${indexes.get(step.onRework) ?? "?"}` : "parar"}</span>
          </div>
        </article>)}
      </CardContent>
    </Card>
  </div>;
}

function PublicationReview({ proposal }: { proposal: PublicationProposal }) {
  return <div className="space-y-4">
    {proposal.repositories.map(repository => <Card key={repository.path} className="gap-0 overflow-hidden border-border bg-card/80 p-0">
      <CardHeader className="flex flex-row items-start gap-3 border-b border-border bg-sidebar/45 p-4">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-md border border-border bg-background"><FolderGit2 className="size-4 text-onedark-cyan" /></div>
        <div className="min-w-0 flex-1"><p className="micro-label text-muted-foreground">Repositório</p><CardTitle className="mt-1 break-all font-mono text-sm">{repository.path === "." ? "Raiz do projeto" : repository.path}</CardTitle></div>
        {repository.branch && <Hint content={repository.branch}><Badge variant="outline" className="max-w-52 shrink truncate font-mono text-[10px] text-primary">{repository.branch}</Badge></Hint>}
      </CardHeader>
      <CardContent className="space-y-4 p-4">
        {repository.reset && <section className="rounded-md border border-onedark-yellow/25 bg-onedark-yellow/5 p-3">
          <p className="micro-label mb-2 flex items-center gap-2 text-onedark-yellow"><RotateCcw className="size-3.5" />Reorganizar histórico</p>
          <code className="block break-all rounded-md border border-border bg-background px-3 py-2 font-mono text-xs text-foreground">git reset --soft {repository.reset.target}</code>
          <p className="mt-2 text-xs leading-5 text-muted-foreground">Move o HEAD para o commit selecionado e mantém as alterações no stage e na pasta de trabalho.</p>
        </section>}
        {repository.branch && <section className="rounded-md border border-border bg-sidebar/55 p-3">
          <p className="micro-label mb-2 flex items-center gap-2 text-muted-foreground"><GitBranch className="size-3.5 text-onedark-cyan" />Branch</p>
          <p className="break-all font-mono text-xs text-foreground">Selecionar se já existir; criar caso contrário: {repository.branch}</p>
        </section>}
        {repository.commitMessage && <section>
          <p className="micro-label mb-2 flex items-center gap-2 text-muted-foreground"><GitCommitHorizontal className="size-3.5 text-onedark-green" />Commit</p>
          <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border bg-background p-3 font-mono text-xs leading-5 text-foreground">{repository.commitMessage}</pre>
        </section>}
        {repository.files.length > 0 && <section>
          <p className="micro-label mb-2 flex items-center gap-2 text-muted-foreground"><FileCode2 className="size-3.5 text-primary" />Arquivos · {repository.files.length}</p>
          <div className="max-h-44 overflow-y-auto rounded-md border border-border bg-background/70 py-1">
            {repository.files.map(file => <p key={file} className="border-b border-border/50 px-3 py-1.5 font-mono text-[11px] last:border-b-0">{file}</p>)}
          </div>
        </section>}
        {repository.push !== "none" && <section className={`rounded-md border p-3 ${repository.push === "force_with_lease" ? "border-onedark-yellow/25 bg-onedark-yellow/5" : "border-onedark-green/25 bg-onedark-green/5"}`}>
          <p className={`micro-label flex items-center gap-2 ${repository.push === "force_with_lease" ? "text-onedark-yellow" : "text-onedark-green"}`}><Upload className="size-3.5" />{repository.push === "force_with_lease" ? "Push com force-with-lease" : "Push para origin"}</p>
          <p className="mt-2 text-xs leading-5 text-muted-foreground">{repository.push === "force_with_lease" ? "Reescreve a branch remota somente se ela não avançou desde a última leitura e configura o upstream." : "Publica o HEAD da branch selecionada e configura o upstream."}</p>
        </section>}
        {repository.pullRequest && <section className="rounded-md border border-primary/20 bg-primary/5 p-3">
          <div className="mb-2 flex flex-wrap items-center gap-2"><p className="micro-label flex items-center gap-2 text-primary"><GitPullRequest className="size-3.5" />Criar ou reutilizar pull request</p><Badge variant="outline" className="font-mono text-[9px]">base: {repository.pullRequest.base}</Badge>{repository.pullRequest.draft && <Badge variant="secondary" className="text-[9px]">Rascunho</Badge>}</div>
          <p className="text-sm font-medium text-foreground">{repository.pullRequest.title}</p>
          <div className="bead-prose mt-3 max-h-64 overflow-y-auto rounded-md border border-border bg-background p-3 text-xs"><LazyChatMarkdown content={repository.pullRequest.body} /></div>
          {repository.pullRequest.merge && <div className="mt-3 flex items-center gap-2 rounded-md border border-onedark-yellow/25 bg-onedark-yellow/5 p-2 text-xs text-onedark-yellow"><GitMerge className="size-3.5" /><span>Merge após localizar ou criar · {repository.pullRequest.merge.method}{repository.pullRequest.merge.deleteBranch ? " · excluir branch" : ""}</span></div>}
        </section>}
      </CardContent>
    </Card>)}
  </div>;
}

export function AuthoringApprovalDrawer({ request, owner, onAnswer }: { request: PendingAuthoring; owner?: string; onAnswer: Decision }) {
  const [note, setNote] = useState("");
  const [pending, setPending] = useState(false);
  const changes = useMemo(() => changedFields(request), [request]);
  const isAgent = request.target.kind === "agent";
  const isPublication = request.target.kind === "publication";
  const action = request.action === "create" ? "Criar" : request.action === "update" ? "Editar" : "Executar";
  const target = isPublication ? "ações" : isAgent ? "agente" : "fluxo";
  const answer = async (approved: boolean) => {
    if (pending) return;
    setPending(true);
    const accepted = await onAnswer(approved, note.trim() || null);
    if (!accepted) setPending(false);
  };
  return <Sheet open onOpenChange={open => { if (!open && !pending) void answer(false); }}>
    <SheetContent showCloseButton={false} className="dark gap-0 border-border bg-background data-[side=right]:w-[min(720px,94vw)] data-[side=right]:sm:max-w-[720px]">
      <SheetHeader className="shrink-0 border-b border-border bg-card/55 p-5 pr-6">
        <div className="mb-3 flex items-center gap-2"><Badge variant="outline" className="gap-1.5 border-primary/30 text-primary">{isPublication ? <Sparkles className="size-3" /> : isAgent ? <Bot className="size-3" /> : <Route className="size-3" />}{action} {target}</Badge>{request.catalogRevision !== null && <Badge variant="secondary" className="font-mono text-[9px]">revisão {request.catalogRevision}</Badge>}{owner && <span className="ml-auto truncate text-xs text-muted-foreground">Solicitado por {owner}</span>}</div>
        <SheetTitle className="text-xl">{isPublication ? "Revisar ações Git e GitHub" : "Revisar alteração no Jarvis"}</SheetTitle>
        <SheetDescription className="mt-1 max-w-2xl leading-5">{request.summary}</SheetDescription>
      </SheetHeader>
      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        <Alert className="mb-5 border-onedark-green/25 bg-onedark-green/5 text-onedark-green">
          <ShieldCheck aria-hidden="true" />
          <AlertTitle>Você mantém o controle</AlertTitle>
          <AlertDescription className="text-foreground">{isPublication ? "Nenhuma operação Git ou GitHub foi executada. O Jarvis realizará somente as ações exibidas após sua aprovação." : "A configuração ainda não foi alterada. O Jarvis salvará esta proposta somente se você aprovar."}</AlertDescription>
        </Alert>
        {request.action === "update" && <section className="mb-5 rounded-md border border-border bg-card/60 p-4">
          <p className="micro-label mb-2 flex items-center gap-2 text-muted-foreground"><GitCompareArrows className="size-3.5" />Campos alterados</p>
          <div className="flex flex-wrap gap-1.5">{changes.length ? changes.map(field => <Badge key={field} variant="outline" className="text-[10px] text-onedark-yellow">{fieldLabels[field] ?? field}</Badge>) : <span className="text-xs text-muted-foreground">A proposta não altera nenhum campo.</span>}</div>
        </section>}
        {request.target.kind === "agent" ? <AgentReview agent={request.target.after} /> : request.target.kind === "flow" ? <FlowReview flow={request.target.after} references={request.agentReferences} /> : <PublicationReview proposal={request.target.after} />}
        <Separator className="my-5" />
        <div className="space-y-2"><Label htmlFor={`authoring-note-${request.toolId}`}>Orientação para o agente <span className="font-normal text-muted-foreground">(opcional)</span></Label><Textarea id={`authoring-note-${request.toolId}`} value={note} onChange={event => setNote(event.target.value)} maxLength={2000} disabled={pending} placeholder="Explique um ajuste se preferir recusar ou deixe uma observação para a aprovação." className="min-h-20 resize-y" /></div>
        <div className="mt-4 flex items-start gap-2 rounded-md border border-border bg-sidebar/55 p-3 text-xs leading-5 text-muted-foreground"><LockKeyhole aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-onedark-yellow" /><span>{isPublication ? "A autorização vale uma vez e somente para os repositórios, arquivos e operações exibidos. Uma falha parcial precisa ser revisada antes de qualquer nova tentativa." : "Agentes e fluxos nativos permanecem protegidos. Esta autorização vale apenas para a proposta exibida, neste turno."}</span></div>
      </div>
      <SheetFooter className="shrink-0 flex-row items-center justify-end gap-3 border-t border-border bg-card/70 p-4">
        <Button variant="outline" className="cursor-pointer" disabled={pending} onClick={() => { void answer(false); }}><X />Recusar</Button>
        <Button className="cursor-pointer" disabled={pending || (request.action === "update" && changes.length === 0)} onClick={() => { void answer(true); }}>{pending ? <Spinner /> : <Sparkles />}{isPublication ? "Aprovar e executar" : "Aprovar e salvar"}</Button>
      </SheetFooter>
    </SheetContent>
  </Sheet>;
}
