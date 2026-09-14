import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, ShieldCheck, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { Skeleton } from "@/components/ui/skeleton";
import { executionGrantListSchema, type ExecutionGrantSummary } from "@/core/execution-grants";
import { libraryError } from "@/core/library";

const SCOPE_LABELS = { conversation: "Conversa", project: "Projeto", repository: "Repositório" } as const;
const DURATION_LABELS = { once: "Uma vez", session: "Sessão atual", until: "Temporária", persistent: "Persistente" } as const;
const MATCH_LABELS = { exact: "Ação exata", commandPrefix: "Mesmo prefixo" } as const;

function effectLabels(grant: ExecutionGrantSummary) {
  const labels: string[] = [];
  if (grant.effects.readsFilesystem) labels.push("Leitura");
  if (grant.effects.writesFilesystem) labels.push("Escrita");
  if (grant.effects.usesNetwork) labels.push("Rede");
  if (grant.effects.controlsProcesses) labels.push("Processos");
  if (grant.effects.destructive) labels.push("Destrutiva");
  if (grant.effects.dynamic) labels.push("Dinâmica");
  if (grant.effects.unknown) labels.push("Desconhecida");
  return labels;
}

function formatDate(value: number | null) {
  return value === null ? "Ainda não utilizada" : new Date(value).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" });
}

export function ExecutionGrantsSettings({ projectId }: { projectId: string }) {
  const [grants, setGrants] = useState<ExecutionGrantSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadedProjectId, setLoadedProjectId] = useState<string | null>(null);
  const [removing, setRemoving] = useState<ExecutionGrantSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);

  useEffect(() => {
    const request = ++generation.current;
    void invoke<unknown>("list_execution_grants", { projectId }).then(value => {
      if (generation.current === request) {
        setGrants(executionGrantListSchema.parse(value));
        setError(null);
        setLoadedProjectId(projectId);
      }
    }).catch(cause => {
      if (generation.current === request) {
        setError(libraryError(cause, "Não foi possível carregar as autorizações de execução."));
        setLoadedProjectId(projectId);
      }
    });
    return () => { if (generation.current === request) generation.current += 1; };
  }, [projectId]);

  const displayedGrants = loadedProjectId === projectId ? grants : null;
  const displayedError = loadedProjectId === projectId ? error : null;

  const revoke = async () => {
    if (!removing || busy) return;
    setBusy(true);
    try {
      await invoke("revoke_execution_grant", { projectId, grantId: removing.id });
      setGrants(current => current?.filter(grant => grant.id !== removing.id) ?? []);
      setRemoving(null);
      toast.success("Autorização de execução revogada");
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível revogar a autorização."));
    } finally {
      setBusy(false);
    }
  };

  return <Card>
    <CardHeader className="border-b border-border">
      <div className="flex items-center gap-2"><ShieldCheck className="size-4 text-onedark-green" /><CardTitle>Autorizações de execução</CardTitle>{displayedGrants && <Badge variant="outline" className="font-mono text-[10px]">{displayedGrants.length}</Badge>}</div>
      <CardDescription>Revise comandos e ferramentas que o Jarvis pode repetir neste projeto sem pedir novamente. Os argumentos completos e o conteúdo dos arquivos não são armazenados neste resumo.</CardDescription>
    </CardHeader>
    <CardContent className="pt-5">
      {displayedError && <Alert variant="destructive"><AlertTriangle /><AlertTitle>Autorizações indisponíveis</AlertTitle><AlertDescription>{displayedError}</AlertDescription></Alert>}
      {!displayedError && displayedGrants === null && <div role="status" aria-label="Carregando autorizações de execução" className="space-y-2"><Skeleton className="h-20 w-full" /><Skeleton className="h-20 w-full" /></div>}
      {!displayedError && displayedGrants?.length === 0 && <div className="rounded-md border border-dashed border-border px-4 py-6 text-center"><p className="text-sm text-foreground">Nenhuma autorização reutilizável</p><p className="mt-1 text-xs text-muted-foreground">As regras criadas durante aprovações aparecerão aqui.</p></div>}
      {!displayedError && displayedGrants && displayedGrants.length > 0 && <div className="space-y-2">{displayedGrants.map(grant => <div key={grant.id} className="flex items-start gap-3 rounded-md border border-border bg-background/45 p-3">
        <div className="min-w-0 flex-1 space-y-2">
          <div className="flex flex-wrap gap-1.5"><Badge variant="outline">{SCOPE_LABELS[grant.scope]}</Badge><Badge variant="outline">{DURATION_LABELS[grant.duration]}</Badge><Badge variant="outline">{MATCH_LABELS[grant.matchKind]}</Badge>{effectLabels(grant).map(label => <Badge key={label} variant="secondary" className="font-normal">{label}</Badge>)}</div>
          <p className="break-all font-mono text-xs text-foreground">{grant.subject}</p>
          {grant.scopeRoot && <p className="break-all font-mono text-[10px] text-muted-foreground">{grant.scopeRoot}</p>}
          <p className="font-mono text-[10px] text-muted-foreground">Usos: {grant.uses} · Último uso: {formatDate(grant.lastUsedAt)}</p>
        </div>
        <Hint content="Revogar autorização"><Button type="button" variant="ghost" size="icon" aria-label={`Revogar autorização ${grant.subject}`} className="size-8 cursor-pointer text-muted-foreground hover:text-destructive" onClick={() => setRemoving(grant)}><Trash2 className="size-3.5" /></Button></Hint>
      </div>)}</div>}
    </CardContent>
    <AlertDialog open={removing !== null} onOpenChange={open => { if (!open && !busy) setRemoving(null); }}>
      <AlertDialogContent className="dark">
        <AlertDialogHeader><AlertDialogTitle>Revogar esta autorização?</AlertDialogTitle><AlertDialogDescription>O Jarvis voltará a pedir confirmação antes de executar esta ação no escopo selecionado.</AlertDialogDescription></AlertDialogHeader>
        {removing && <p className="break-all rounded-md border border-border bg-background p-2 font-mono text-xs">{removing.subject}</p>}
        <AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={busy}>Manter</AlertDialogCancel><AlertDialogAction data-confirm-action variant="destructive" className="cursor-pointer" disabled={busy} onClick={event => { event.preventDefault(); void revoke(); }}>{busy ? "Revogando…" : "Revogar"}</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </Card>;
}
