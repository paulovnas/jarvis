import { lazy, Suspense, useState } from "react";
import { Monitor, RefreshCw, Settings2, Square, X } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Hint } from "@/components/ui/hint";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { RowsSkeleton } from "@/components/layout/LoadingSkeletons";
import { computerAccessLabels, computerModeDescriptions, computerUseError, type ComputerAccess } from "@/core/computer-use";
import { openAuxiliaryWindow } from "@/core/auxiliary-windows";
import { useComputerUse } from "@/hooks/use-computer-use";

const accessItems = Object.entries(computerAccessLabels).map(([value, label]) => ({ value: value as ComputerAccess, label }));
const phaseLabels = { observing: "Observando", prepared: "Ação preparada", inputSent: "Entrada enviada", observed: "Observado", uncertain: "Resultado incerto", stopped: "Interrompido", error: "Falha" };
const SettingsDialog = lazy(() => import("@/components/settings/SettingsDialog").then(module => ({ default: module.SettingsDialog })));

export function ComputerUseSession({ conversationId, onStop }: { conversationId: string; onStop: () => Promise<void> }) {
  const computer = useComputerUse(conversationId);
  const [open, setOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [dismissedIssue, setDismissedIssue] = useState<string | null>(null);
  const [appId, setAppId] = useState<string | null>(null);
  const [access, setAccess] = useState<ComputerAccess>("observe");
  const snapshot = computer.snapshot;
  const app = computer.apps?.find(item => item.appId === appId);
  const capabilities = snapshot?.capabilities;
  const canObserve = !!capabilities?.supported && (capabilities.accessibility || capabilities.screenCapture && capabilities.captureSupported);
  const canControl = !!capabilities?.supported && capabilities.accessibility && capabilities.inputPosting;
  const needsPermissions = !!capabilities?.supported && (!canControl || capabilities.captureSupported && !capabilities.screenCapture);
  const setupIssue = snapshot?.activity?.phase === "error" && needsPermissions ? snapshot.activity.operationId : null;
  const visible = open || setupIssue !== null && setupIssue !== dismissedIssue;
  const manual = snapshot?.mode !== "yolo";
  const configure = async () => {
    try {
      if (!await openAuxiliaryWindow("settings", "computer")) setSettingsOpen(true);
      setOpen(false);
      setDismissedIssue(setupIssue);
    } catch (cause) { toast.error(computerUseError(cause)); }
  };
  const stop = async () => {
    const results = await Promise.allSettled([computer.stop(), onStop()]);
    if (results[1].status === "rejected") toast.error(computerUseError(results[1].reason));
  };
  const nameFor = (bundleId: string) => computer.apps?.find(item => item.identity.bundleId === bundleId)?.name ?? bundleId;

  return <><Popover open={visible} onOpenChange={next => { setOpen(next); if (!next) setDismissedIssue(setupIssue); if (next && capabilities?.supported && manual) void computer.loadApps(); }}>
      <Hint content="Uso do computador"><PopoverTrigger render={<Button type="button" variant="ghost" size="icon" className="relative size-7 cursor-pointer text-muted-foreground hover:text-foreground" aria-label="Usar computador" />}><Monitor className="size-3.5" />{snapshot?.activity && !snapshot.stopped && <span aria-hidden="true" className="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-primary" />}</PopoverTrigger></Hint>
    <PopoverContent side="top" align="start" className="w-[min(420px,calc(100vw-2rem))] max-h-[65dvh] overflow-y-auto p-4">
    <div className="mb-3 flex min-w-0 flex-wrap items-center gap-2"><h2 className="text-sm font-medium">Uso do computador</h2>{snapshot && <Badge variant="outline" className="text-[10px]">{manual ? "Manual" : "YOLO"}</Badge>}</div>
    <div className="mb-3 flex min-w-0 flex-wrap items-center gap-2">
      {snapshot?.stopped ? <p role="status" className="text-xs text-muted-foreground">Uso do computador interrompido</p> : snapshot?.activity && <p role="status" aria-live="polite" className={`min-w-0 flex-1 text-xs ${["uncertain", "error"].includes(snapshot.activity.phase) ? "text-onedark-yellow" : "text-muted-foreground"}`}>{snapshot.activity.message}<span className="ml-2 font-mono text-[10px]">{phaseLabels[snapshot.activity.phase]}</span></p>}
      {!!snapshot && snapshot.activity && !snapshot.stopped && <Button type="button" variant="ghost" size="sm" aria-label="Interromper uso do computador" className="ml-auto cursor-pointer gap-1.5 text-xs" disabled={computer.busy === "stop_computer_use"} onClick={() => void stop()}><Square className="size-3 text-onedark-red" />Interromper</Button>}
    </div>
      <div className="space-y-3">
        <p className="text-xs leading-5 text-muted-foreground">{computerModeDescriptions[snapshot?.mode ?? "manual"]}</p>
        {computer.error && <p role="alert" className="text-xs text-destructive">{computer.error}</p>}
        {!snapshot && !computer.error && <RowsSkeleton label="Carregando uso do computador" count={2} />}
        {capabilities && !capabilities.supported && <p role="status" className="text-xs">{capabilities.reason ?? "O uso nativo do computador está disponível apenas no macOS."}</p>}
        {capabilities?.supported && capabilities.reason && <p role="status" className="text-xs text-onedark-yellow">{capabilities.reason}</p>}
        {needsPermissions && <div className="rounded-md bg-secondary p-3"><p className="mb-2 text-xs leading-5">Para usar o computador, conceda as permissões do macOS ao Jarvis. O agente aguarda essa configuração antes de controlar os aplicativos.</p><Button type="button" variant="outline" size="sm" className="cursor-pointer text-xs" onClick={() => void configure()}><Settings2 className="size-3.5" />Configurar permissões</Button></div>}
        {snapshot && capabilities?.supported && manual && <>
          <div className="flex flex-wrap items-end gap-2">
            <div className="min-w-40 flex-1 space-y-1"><Label htmlFor={`computer-app-${conversationId}`} className="text-xs">Aplicativo</Label><Select items={(computer.apps ?? []).map(item => ({ value: item.appId, label: item.name }))} value={app?.appId ?? null} onValueChange={setAppId} disabled={!!computer.busy}><SelectTrigger id={`computer-app-${conversationId}`} aria-label="Aplicativo" className="w-full cursor-pointer text-xs"><SelectValue placeholder="Escolha um aplicativo">{app?.name}</SelectValue></SelectTrigger><SelectContent>{computer.apps?.map(item => <SelectItem key={item.appId} value={item.appId} className="cursor-pointer">{item.name}</SelectItem>)}</SelectContent></Select></div>
            <div className="min-w-40 space-y-1"><Label htmlFor={`computer-access-${conversationId}`} className="text-xs">Acesso</Label><Select items={accessItems} value={access} onValueChange={value => { if (value) setAccess(value); }} disabled={!!computer.busy}><SelectTrigger id={`computer-access-${conversationId}`} aria-label="Acesso ao aplicativo" className="w-full cursor-pointer text-xs"><SelectValue>{computerAccessLabels[access]}</SelectValue></SelectTrigger><SelectContent><SelectItem value="observe" className="cursor-pointer">Observar</SelectItem><SelectItem value="control" disabled={!canControl} className="cursor-pointer">Observar e controlar</SelectItem></SelectContent></Select></div>
            <Button type="button" size="sm" variant="outline" className="cursor-pointer text-xs" disabled={!app || !!computer.busy || (access === "control" ? !canControl : !canObserve)} onClick={() => { if (app) void computer.authorize(app.appId, access).then(accepted => { if (accepted) toast.success("Aplicativo autorizado nesta conversa"); }); }}>Autorizar aplicativo</Button>
          </div>
          {computer.apps?.length === 0 && <p role="status" className="text-xs text-muted-foreground">Nenhum aplicativo com janela disponível.</p>}
          {snapshot.grants.map(grant => <div key={grant.id} className="flex items-center gap-2 border-t border-border pt-2"><div className="min-w-0 flex-1"><p className="truncate text-xs font-medium">{nameFor(grant.identity.bundleId)}</p><p className="truncate font-mono text-[10px] text-muted-foreground">{grant.identity.bundleId}</p></div><Badge variant="outline" className="text-[10px] font-normal">{computerAccessLabels[grant.access]}</Badge><Button type="button" size="icon-sm" variant="ghost" className="cursor-pointer text-muted-foreground" disabled={!!computer.busy} aria-label={`Revogar acesso a ${nameFor(grant.identity.bundleId)}`} onClick={() => void computer.revoke(grant.id).then(accepted => { if (accepted) toast.success("Acesso revogado"); })}><X className="size-3.5" /></Button></div>)}
        </>}
        {snapshot?.targets.map(target => <div key={target.id} className="border-t border-border pt-2 text-xs"><p className="font-medium">{target.appName}</p><p className="mt-0.5 text-muted-foreground">{target.windowTitle || "Janela sem título"}</p></div>)}
        <div className="flex flex-wrap gap-2"><Button type="button" variant="ghost" size="sm" className="cursor-pointer text-xs text-muted-foreground" disabled={!!computer.busy} onClick={() => { void computer.refresh().then(accepted => { if (accepted && visible && capabilities?.supported && manual) void computer.loadApps(); }); }}><RefreshCw className="size-3" />Atualizar estado</Button>{!needsPermissions && <Button type="button" variant="ghost" size="sm" className="cursor-pointer text-xs text-muted-foreground" onClick={() => void configure()}><Settings2 className="size-3" />Configurar</Button>}</div>
      </div>
    </PopoverContent>
  </Popover>{settingsOpen && <Suspense fallback={<RowsSkeleton label="Abrindo configurações do computador" count={3} />}><SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} initialSection="computer" /></Suspense>}</>;
}
