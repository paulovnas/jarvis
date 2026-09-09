import { useEffect, useRef, useState } from "react";
import { Check, CircleAlert, KeyRound, RefreshCw, ShieldCheck, Wrench } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { AlertDialog, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import { Skeleton } from "@/components/ui/skeleton";
import { CoreInstallProgress } from "@/components/core/CoreInstallProgress";
import { Context7Configuration } from "@/components/settings/CoreSettings";
import { CORE_DETAILS } from "@/core/core-presentation";
import type { CoreId } from "@/core/core-components";
import type { CoreController } from "@/hooks/use-core";

export default function CoreDiagnostics({ core, onClose }: { core: CoreController; onClose: () => void }) {
  const [reinstall, setReinstall] = useState<CoreId | null>(null);
  const [configuring, setConfiguring] = useState(false);
  const started = useRef(false);
  const { snapshot, busy, diagnose, repair } = core;
  useEffect(() => { if (!started.current && !busy) { started.current = true; void diagnose(); } }, [busy, diagnose]);
  const target = snapshot?.items.find(item => item.id === reinstall);
  return <>
    <Dialog open onOpenChange={open => { if (!open && !busy) onClose(); }}>
      <DialogContent showCloseButton={!busy} className="dark instrument-panel flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
        <DialogHeader className="shrink-0 border-b border-border px-6 py-5"><DialogTitle className="flex items-center gap-2 text-base"><Wrench className="size-4 text-primary" />Diagnóstico e Reparo</DialogTitle><DialogDescription>Verificação local dos cinco componentes do Jarvis.</DialogDescription></DialogHeader>
        <div className="min-h-0 overflow-y-auto p-6">
          {core.error && <p role="alert" className="mb-4 text-sm text-destructive">{core.error}</p>}
          <div className="mb-4 flex items-center justify-between gap-3"><Badge variant="outline" className={snapshot?.ready ? "border-onedark-green/30 text-onedark-green" : "border-onedark-yellow/30 text-onedark-yellow"}>{snapshot?.ready ? "Core pronto" : "Atenção necessária"}</Badge><Button size="sm" variant="ghost" disabled={busy} onClick={() => void diagnose()}><RefreshCw className="size-3.5" />Analisar novamente</Button></div>
          <div className="grid gap-3 sm:grid-cols-2">
            {!snapshot && Array.from({ length: 5 }, (_, i) => <Skeleton key={i} className="h-40" />)}
            {snapshot?.items.map(item => {
              const { icon: Icon, color, tint } = CORE_DETAILS[item.id];
              const ready = item.installed && item.configured && !item.healthError;
              return <Card key={item.id} className="instrument-panel gap-0 py-0"><CardContent className="p-4">
                <div className="flex items-center gap-3"><div className={`flex size-8 items-center justify-center rounded-md border ${tint} ${color}`}><Icon className="size-4" /></div><div className="min-w-0 flex-1"><h2 className="text-sm font-medium">{item.name}</h2><p className="font-mono text-[10px] text-muted-foreground">{item.installedVersion ? `v${item.installedVersion}` : "Não instalado"}</p></div>{ready ? <ShieldCheck aria-label="Pronto" className="size-4 text-onedark-green" /> : <CircleAlert aria-label="Precisa de atenção" className="size-4 text-onedark-yellow" />}</div>
                {item.stage ? <CoreInstallProgress item={item} /> : <div className="mt-4 space-y-2">
                  {item.diagnostics.length ? item.diagnostics.map(check => <div key={check.label} className="flex items-start gap-2 text-[11px]">{check.passed ? <Check className="mt-0.5 size-3 shrink-0 text-onedark-green" /> : <CircleAlert className="mt-0.5 size-3 shrink-0 text-onedark-red" />}<div><p className="font-medium">{check.label}</p>{!check.passed && <p className="mt-1 text-muted-foreground">{check.message}</p>}</div></div>) : <p className="text-xs text-muted-foreground">{item.healthError ?? item.error ?? (ready ? "Aguardando análise" : item.installed ? "Configuração pendente" : "Instalação pendente")}</p>}
                  {item.error && item.diagnostics.length > 0 && <p role="alert" className="text-xs text-destructive">{item.error}</p>}
                </div>}
                {!ready && <div className="mt-4 flex flex-wrap gap-2 border-t border-border pt-3">
                  <Button size="sm" variant="outline" disabled={busy} aria-label={`Reparar ${item.name}`} onClick={() => void repair(item.id, false)}><Wrench className="size-3" />Reparar</Button>
                  <Button size="sm" variant="ghost" disabled={busy} aria-label={`Reinstalar ${item.name}`} onClick={() => setReinstall(item.id)}>Reinstalar</Button>
                  {item.id === "context7" && item.installed && <Button size="sm" variant="ghost" disabled={busy} onClick={() => setConfiguring(true)}><KeyRound className="size-3" />Configurar Context7</Button>}
                </div>}
              </CardContent></Card>;
            })}
          </div>
        </div>
        <div className="flex shrink-0 justify-end border-t border-border px-6 py-4"><Button variant={snapshot?.ready ? "default" : "outline"} disabled={busy} onClick={onClose}>{snapshot?.ready ? "Voltar ao Jarvis" : "Fechar"}</Button></div>
      </DialogContent>
    </Dialog>
    <Context7Configuration open={configuring} onOpenChange={setConfiguring} onSaved={core.refresh} />
    <AlertDialog open={reinstall !== null} onOpenChange={open => { if (!open) setReinstall(null); }}><AlertDialogContent className="dark"><AlertDialogHeader><AlertDialogTitle>Reinstalar {target?.name}?</AlertDialogTitle><AlertDialogDescription>O pacote atual será removido após baixar e verificar a nova instalação. Projetos, conversas e chaves serão preservados.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><Button variant="outline" onClick={() => setReinstall(null)}>Cancelar</Button><Button data-confirm-action variant="destructive" onClick={() => { if (reinstall) void repair(reinstall, true); setReinstall(null); }}>Confirmar reinstalação</Button></AlertDialogFooter></AlertDialogContent></AlertDialog>
  </>;
}
