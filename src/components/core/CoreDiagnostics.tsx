import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, CircleAlert, Download, KeyRound, RefreshCw, Wrench } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { RowsSkeleton } from "@/components/layout/LoadingSkeletons";
import { CoreInstallProgress } from "@/components/core/CoreInstallProgress";
import { CoreReinstallDialog } from "@/components/core/CoreReinstallDialog";
import { BrowserRuntimeRow } from "@/components/core/BrowserRuntimeRow";
import { DiagnosticEvidence } from "@/components/core/DiagnosticEvidence";
import { Context7Configuration } from "@/components/settings/CoreSettings";
import { OpenMontageConfiguration } from "@/components/settings/OpenMontageConfiguration";
import { CORE_DETAILS } from "@/core/core-presentation";
import { ACTIVE_CORE_IDS, activeCore, type CoreId } from "@/core/core-components";
import type { CoreController } from "@/hooks/use-core";

export default function CoreDiagnostics({ core, onClose }: { core: CoreController; onClose: () => void }) {
  const [reinstall, setReinstall] = useState<CoreId | null>(null);
  const [configuring, setConfiguring] = useState(false);
  const [configuringVideo, setConfiguringVideo] = useState(false);
  const [expanded, setExpanded] = useState<CoreId | null>(null);
  const started = useRef(false);
  const { snapshot, busy, diagnose, repair, install } = core;
  useEffect(() => { if (snapshot && !started.current && !busy && snapshot.items.some(item => activeCore(item.id) && !item.diagnostics.length)) { started.current = true; void diagnose(); } }, [snapshot, busy, diagnose]);
  const target = snapshot?.items.find(item => item.id === reinstall);
  const hasPendingError = snapshot?.items.some(item => item.error) === true || snapshot?.browserRuntime?.state === "unavailable";
  const blocking = busy && !snapshot?.items.some(item => item.stage === "Analisando componente");
  return <>
    <Dialog open onOpenChange={open => { if (!open && !blocking) onClose(); }}>
      <DialogContent showCloseButton={!blocking} className="dark instrument-panel flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
        <DialogHeader className="shrink-0 border-b border-border px-5 py-4"><DialogTitle className="flex items-center gap-2 text-base"><Wrench className="size-4 text-primary" />Diagnóstico e Reparo</DialogTitle><DialogDescription>Verificação local dos componentes do Jarvis.</DialogDescription></DialogHeader>
        <div className="min-h-0 overflow-y-auto p-5">
          {core.error && <p role="alert" className="mb-4 text-sm text-destructive">{core.error}</p>}
          <div className="mb-4 flex items-center justify-between gap-3"><Badge variant="outline" className={snapshot?.ready && !hasPendingError ? "border-onedark-green/30 text-onedark-green" : "border-onedark-yellow/30 text-onedark-yellow"}>{snapshot?.ready ? hasPendingError ? "Core funcional · ação pendente" : "Core pronto" : "Atenção necessária"}</Badge><Button size="sm" variant="ghost" disabled={busy} onClick={() => void diagnose()}><RefreshCw className="size-3.5" />Analisar novamente</Button></div>
          <Card className="instrument-panel gap-0 py-0"><CardContent className="divide-y divide-border p-0">
            {!snapshot && <RowsSkeleton label="Carregando diagnóstico" count={ACTIVE_CORE_IDS.length} />}
            {snapshot?.browserRuntime && <BrowserRuntimeRow runtime={snapshot.browserRuntime} />}
            {snapshot?.items.filter(item => activeCore(item.id)).map(item => {
              const { icon: Icon, color, summary, description } = CORE_DETAILS[item.id];
              const ready = item.installed && item.configured && !item.healthError;
              const missing = !item.installed && !item.installedVersion;
              const open = expanded === item.id;
              const passed = item.diagnostics.filter(check => check.passed);
              const failed = item.diagnostics.filter(check => !check.passed);
              const issue = item.healthError ?? item.error;
              return <Collapsible key={item.id} open={open} onOpenChange={next => setExpanded(next ? item.id : null)} className="min-w-0 px-4 py-3">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                  <Icon aria-hidden="true" className={`size-4 shrink-0 ${color}`} />
                  <div className="min-w-0 flex-1 basis-48">
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5"><h2 className="text-sm font-medium">{item.name}</h2><span className="font-mono text-[10px] text-muted-foreground">{item.installedVersion ? `v${item.installedVersion}` : "Não instalado"}</span></div>
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">{summary}</p>
                    {item.bundled && <p className="text-xs leading-5 text-muted-foreground">Incluído no Jarvis; atualizado com o aplicativo.</p>}
                  </div>
                  <div className="ml-auto flex items-center gap-2">
                    {ready && !item.error && !failed.length ? <span className="text-xs text-muted-foreground">Pronto</span> : <span className="flex items-center gap-1 text-xs text-onedark-yellow"><CircleAlert aria-hidden="true" className="size-3.5" />Atenção</span>}
                    <CollapsibleTrigger render={<Button size="sm" variant="ghost" />} aria-label={`Verificações de ${item.name}`} className="cursor-pointer gap-2 text-muted-foreground">{item.diagnostics.length > 0 ? <span className="font-mono text-[10px]">{passed.length}/{item.diagnostics.length}</span> : <span className="text-xs">Detalhes</span>}<ChevronDown aria-hidden="true" className={`size-3.5 ${open ? "rotate-180" : ""}`} /></CollapsibleTrigger>
                  </div>
                </div>
                {item.stage ? <CoreInstallProgress item={item} onCancel={() => void core.cancel(item.id)} cancelling={core.cancellingId === item.id} /> : <>
                  {failed.length > 0 && <div className="ml-7 mt-2 space-y-2">{failed.map(check => <div key={check.label} className="text-xs"><p className="font-medium text-destructive">{check.label}</p><p className="mt-0.5 leading-5 text-muted-foreground">{check.message}</p></div>)}</div>}
                  {issue && <p role="alert" className="ml-7 mt-2 text-xs text-destructive">{issue}</p>}
                  {!item.diagnostics.length && !issue && <p className="ml-7 mt-2 text-xs text-muted-foreground">{ready ? "Aguardando análise" : item.installed ? "Configuração pendente" : "Instalação pendente"}</p>}
                </>}
                {!item.bundled && (!ready || item.error || failed.length > 0) && <div className="ml-7 mt-2 flex flex-wrap gap-2">
                  {missing ? <Button size="sm" variant="outline" disabled={busy} aria-label={`Instalar ${item.name}`} onClick={() => void install([item.id])}><Download className="size-3" />Instalar</Button> : <>
                    {(!ready || failed.length > 0) && <Button size="sm" variant="outline" disabled={busy} aria-label={`Reparar ${item.name}`} onClick={() => void repair(item.id, false)}><Wrench className="size-3" />Reparar</Button>}
                    <Button size="sm" variant="ghost" disabled={busy} aria-label={`Reinstalar ${item.name}`} onClick={() => setReinstall(item.id)}>Reinstalar</Button>
                  </>}
                  {item.id === "context7" && item.installed && <Button size="sm" variant="ghost" disabled={busy} onClick={() => setConfiguring(true)}><KeyRound className="size-3" />Configurar Context7</Button>}
                </div>}
                {item.bundled && (!ready || issue || failed.length > 0) && <p className="ml-7 mt-2 text-xs text-muted-foreground">Atualize ou reinstale o Jarvis para recuperar este componente nativo.</p>}
                {open && <CollapsibleContent className="ml-7 space-y-3 pt-3">
                  <p className="max-w-prose text-xs leading-5 text-muted-foreground">{description}</p>
                  {passed.length > 0 && <ul aria-label={`Verificações concluídas de ${item.name}`} className="space-y-1.5">{passed.map(check => <li key={check.label} className="flex items-center gap-2 text-xs text-muted-foreground"><Check aria-hidden="true" className="size-3 shrink-0" />{check.label}</li>)}</ul>}
                  {item.id === "openmontage" && item.installed && <Button size="sm" variant="ghost" disabled={busy} onClick={() => setConfiguringVideo(true)} className="cursor-pointer"><KeyRound className="size-3" />Configurar OpenMontage</Button>}
                </CollapsibleContent>}
              </Collapsible>;
            })}
          </CardContent></Card>
          <DiagnosticEvidence />
        </div>
        <div className="flex shrink-0 justify-end border-t border-border px-5 py-3"><Button variant={snapshot?.ready ? "default" : "outline"} disabled={blocking} onClick={onClose}>{snapshot?.ready ? "Voltar ao Jarvis" : "Fechar"}</Button></div>
      </DialogContent>
    </Dialog>
    <Context7Configuration open={configuring} onOpenChange={setConfiguring} onSaved={core.refresh} />
    <OpenMontageConfiguration open={configuringVideo} onOpenChange={setConfiguringVideo} onSaved={core.refresh} />
    <CoreReinstallDialog name={target?.name} open={reinstall !== null} busy={busy} onOpenChange={open => { if (!open) setReinstall(null); }} onConfirm={() => { if (reinstall) void repair(reinstall, true); setReinstall(null); }} />
  </>;
}
