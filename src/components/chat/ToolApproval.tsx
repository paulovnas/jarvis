import { useState } from "react";
import { AlertTriangle, Network, ShieldAlert, ShieldCheck } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { ApprovalDecision, PendingApproval } from "@/core/chat";

type Grant = NonNullable<ApprovalDecision["grant"]>;
type GrantScope = Grant["scope"];
type GrantDuration = Grant["duration"];
type GrantMatch = Grant["matchKind"];

const ARGUMENT_LABELS: Record<string, string> = {
  path: "Arquivo",
  command: "Comando",
  content: "Conteúdo proposto",
  oldText: "Trecho original",
  newText: "Substituição",
  timeoutSeconds: "Tempo limite (segundos)",
  id: "Terminal",
  reason: "Motivo",
};

const SCOPE_LABELS: Record<GrantScope, string> = {
  conversation: "Esta conversa",
  project: "Este projeto",
  repository: "Este repositório",
};

const DURATION_LABELS: Record<GrantDuration, string> = {
  session: "Até fechar o Jarvis",
  persistent: "Manter entre sessões",
};

const MATCH_LABELS: Record<GrantMatch, string> = {
  exact: "Somente esta ação",
  commandPrefix: "Comandos com o mesmo prefixo",
};

const SCOPE_ITEMS = Object.entries(SCOPE_LABELS).map(([value, label]) => ({ value: value as GrantScope, label }));
const DURATION_ITEMS = Object.entries(DURATION_LABELS).map(([value, label]) => ({ value: value as GrantDuration, label }));
const MATCH_ITEMS = Object.entries(MATCH_LABELS).map(([value, label]) => ({ value: value as GrantMatch, label }));

const BACKEND_LABELS = {
  macosSeatbelt: "Seatbelt do macOS",
  linuxBubblewrap: "Bubblewrap do Linux",
  windowsJobObject: "Job Object do Windows",
  native: "Execução nativa",
} as const;

const AVAILABILITY_LABELS = {
  full: "isolamento completo",
  partial: "isolamento parcial",
  unavailable: "sem isolamento",
} as const;

function subject(name: string) {
  if (name.startsWith("browser_")) return "ação no navegador";
  if (name.startsWith("beads_")) return "alteração de tarefa";
  if (name === "bash") return "comando";
  if (name === "terminal_close") return "fechamento de terminal";
  if (name === "terminal_start") return "abertura de terminal";
  if (name === "process_start") return "processo persistente";
  return "alteração de arquivo";
}

function effectLabels(effects: NonNullable<PendingApproval["policy"]>["effects"]) {
  const labels: string[] = [];
  if (effects.readsFilesystem) labels.push("Lê arquivos");
  if (effects.writesFilesystem) labels.push("Escreve arquivos");
  if (effects.usesNetwork) labels.push("Usa rede");
  if (effects.controlsProcesses) labels.push("Controla processos");
  if (effects.destructive) labels.push("Pode remover dados");
  if (effects.dynamic) labels.push("Comando dinâmico");
  if (effects.unknown) labels.push("Efeito desconhecido");
  return labels;
}

function formattedCommand(request: PendingApproval) {
  const command = request.policy?.command;
  if (!command) return null;
  return command.invocations.map(invocation => invocation.argv.join(" ")).join(" | ");
}

export function ToolApproval({ request, projectPath, onAnswer }: {
  request: PendingApproval;
  projectPath: string;
  onAnswer: (decision: ApprovalDecision) => Promise<boolean>;
}) {
  const [pending, setPending] = useState(false);
  const [reuse, setReuse] = useState(false);
  const [scope, setScope] = useState<GrantScope>("conversation");
  const [duration, setDuration] = useState<GrantDuration>("session");
  const [matchKind, setMatchKind] = useState<GrantMatch>("exact");
  const policy = request.policy;
  const command = formattedCommand(request);
  const effects = policy ? effectLabels(policy.effects) : [];

  const setGrantScope = (next: GrantScope | null) => {
    if (!next) return;
    setScope(next);
    if (next === "conversation") setDuration("session");
  };

  const answer = async (decision: ApprovalDecision) => {
    if (pending) return;
    setPending(true);
    const accepted = await onAnswer(decision);
    if (!accepted) setPending(false);
  };

  const approve = () => {
    const grant = reuse && policy ? { scope, duration, matchKind } : null;
    void answer({ approved: true, grant });
  };

  return <Card role="region" aria-label="Autorização de ferramenta" className="mb-3 border border-onedark-yellow/40 bg-onedark-yellow/[0.035]" size="sm">
    <CardHeader>
      <div className="flex items-start gap-3">
        <div className="flex size-8 shrink-0 items-center justify-center rounded-md border border-onedark-yellow/25 bg-onedark-yellow/10 text-onedark-yellow">
          <ShieldAlert aria-hidden="true" className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <CardTitle>Autorizar {subject(request.tool.name)}?</CardTitle>
          <CardDescription className="mt-1 break-all font-mono text-[10px]">{policy?.workingDirectory ?? projectPath}</CardDescription>
        </div>
      </div>
    </CardHeader>
    <CardContent className="space-y-4">
      {policy && <Alert className="border-onedark-yellow/20 bg-background/45">
        <AlertTriangle className="text-onedark-yellow" />
        <AlertTitle>{policy.reason}</AlertTitle>
        <AlertDescription>O Jarvis classificou esta ação como <span className="font-mono">{policy.code}</span>.</AlertDescription>
      </Alert>}

      {command && <div className="space-y-1.5">
        <p className="micro-label">Comando interpretado</p>
        <pre className="max-h-44 overflow-auto whitespace-pre-wrap break-all rounded-md border border-border bg-background p-2 font-mono text-xs">{command}</pre>
      </div>}

      {!command && Object.entries(request.tool.args).map(([key, value]) => <div key={key} className="space-y-1.5">
        <p className="micro-label">{ARGUMENT_LABELS[key] ?? key}</p>
        <pre className="max-h-44 overflow-auto whitespace-pre-wrap break-all rounded-md border border-border bg-background p-2 font-mono text-xs">{typeof value === "string" ? value : JSON.stringify(value, null, 2)}</pre>
      </div>)}

      {policy && <div className="space-y-3 rounded-md border border-border bg-background/45 p-3">
        <div className="flex flex-wrap gap-1.5">
          {effects.map(label => <Badge key={label} variant="outline" className="font-normal">{label}</Badge>)}
          {!effects.length && <Badge variant="outline" className="font-normal">Sem efeitos materiais detectados</Badge>}
        </div>
        {(policy.readPaths.length > 0 || policy.writePaths.length > 0) && <div className="grid gap-2 text-[11px] text-muted-foreground sm:grid-cols-2">
          {policy.readPaths.length > 0 && <div><p className="micro-label mb-1">Leitura</p>{policy.readPaths.map(path => <p key={path} className="break-all font-mono">{path}</p>)}</div>}
          {policy.writePaths.length > 0 && <div><p className="micro-label mb-1">Escrita</p>{policy.writePaths.map(path => <p key={path} className="break-all font-mono">{path}</p>)}</div>}
        </div>}
        {policy.sandbox && <div className="flex items-start gap-2 border-t border-border pt-3 text-[11px] text-muted-foreground">
          {policy.sandbox.availability === "full" ? <ShieldCheck aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-onedark-green" /> : <Network aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-onedark-yellow" />}
          <div>
            <p><span className="text-foreground">{BACKEND_LABELS[policy.sandbox.backend]}</span> · {AVAILABILITY_LABELS[policy.sandbox.availability]}</p>
            <p>Rede: {policy.sandbox.network === "isolated" ? "isolada" : policy.sandbox.network === "allowed" ? "permitida" : "nativa"}. Árvore de processos: {policy.sandbox.processTreeIsolated ? "isolada" : "nativa"}.</p>
            {policy.sandbox.reason && <p className="mt-1 text-onedark-yellow">{policy.sandbox.reason}</p>}
          </div>
        </div>}
      </div>}

      {policy && <div className="space-y-3 rounded-md border border-border p-3">
        <div className="flex items-center justify-between gap-3">
          <div><Label htmlFor={`reuse-${request.tool.id}`} className="cursor-pointer">Lembrar esta autorização</Label><p className="mt-0.5 text-[11px] text-muted-foreground">Crie uma regra auditável para ações equivalentes.</p></div>
          <Switch id={`reuse-${request.tool.id}`} checked={reuse} onCheckedChange={setReuse} disabled={pending} className="cursor-pointer" />
        </div>
        {reuse && <div className="grid gap-3 border-t border-border pt-3 sm:grid-cols-3">
          <div className="space-y-1.5"><Label htmlFor={`scope-${request.tool.id}`}>Escopo</Label><Select items={SCOPE_ITEMS} value={scope} onValueChange={setGrantScope} disabled={pending}>
            <SelectTrigger id={`scope-${request.tool.id}`} aria-label="Escopo da autorização" className="w-full cursor-pointer"><SelectValue>{SCOPE_LABELS[scope]}</SelectValue></SelectTrigger>
            <SelectContent>
              <SelectItem value="conversation" className="cursor-pointer">{SCOPE_LABELS.conversation}</SelectItem>
              <SelectItem value="project" className="cursor-pointer">{SCOPE_LABELS.project}</SelectItem>
              <SelectItem value="repository" disabled={!policy.repositoryRoot} className="cursor-pointer">{SCOPE_LABELS.repository}</SelectItem>
            </SelectContent>
          </Select></div>
          <div className="space-y-1.5"><Label htmlFor={`duration-${request.tool.id}`}>Duração</Label><Select items={DURATION_ITEMS} value={duration} onValueChange={value => { if (value) setDuration(value); }} disabled={pending || scope === "conversation"}>
            <SelectTrigger id={`duration-${request.tool.id}`} aria-label="Duração da autorização" className="w-full cursor-pointer"><SelectValue>{DURATION_LABELS[duration]}</SelectValue></SelectTrigger>
            <SelectContent><SelectItem value="session" className="cursor-pointer">{DURATION_LABELS.session}</SelectItem><SelectItem value="persistent" className="cursor-pointer">{DURATION_LABELS.persistent}</SelectItem></SelectContent>
          </Select></div>
          <div className="space-y-1.5"><Label htmlFor={`match-${request.tool.id}`}>Correspondência</Label><Select items={MATCH_ITEMS} value={matchKind} onValueChange={value => { if (value) setMatchKind(value); }} disabled={pending}>
            <SelectTrigger id={`match-${request.tool.id}`} aria-label="Correspondência da autorização" className="w-full cursor-pointer"><SelectValue>{MATCH_LABELS[matchKind]}</SelectValue></SelectTrigger>
            <SelectContent><SelectItem value="exact" className="cursor-pointer">{MATCH_LABELS.exact}</SelectItem><SelectItem value="commandPrefix" disabled={!policy.commandPrefixAvailable} className="cursor-pointer">{MATCH_LABELS.commandPrefix}</SelectItem></SelectContent>
          </Select></div>
        </div>}
        {scope === "repository" && policy.repositoryRoot && <p className="break-all font-mono text-[10px] text-muted-foreground">{policy.repositoryRoot}</p>}
      </div>}
    </CardContent>
    <CardFooter className="justify-end gap-2">
      <Button variant="outline" className="cursor-pointer" disabled={pending} onClick={() => { void answer({ approved: false, grant: null }); }}>Recusar</Button>
      <Button className="cursor-pointer" disabled={pending} onClick={approve}>{reuse ? "Autorizar e lembrar" : "Autorizar uma vez"}</Button>
    </CardFooter>
  </Card>;
}
