import { lazy, Suspense, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ChevronDown, Cpu, Download, ExternalLink, KeyRound, RefreshCw, Wrench } from "lucide-react";
import { CORE_DETAILS } from "@/core/core-presentation";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Skeleton } from "@/components/ui/skeleton";
import { RowsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Progress } from "@/components/ui/progress";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ACTIVE_CORE_IDS, activeCore, coreError, optionalCore, type CoreId } from "@/core/core-components";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Input } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { useCore, type CoreController } from "@/hooks/use-core";
import { CoreInstallProgress } from "@/components/core/CoreInstallProgress";
import { CoreReinstallDialog } from "@/components/core/CoreReinstallDialog";
import { BrowserRuntimeRow } from "@/components/core/BrowserRuntimeRow";
import { Hint } from "@/components/ui/hint";
import { OpenMontageConfiguration } from "./OpenMontageConfiguration";

export function CorePanel({ core, setup = false }: { core: CoreController; setup?: boolean }) {
  const { snapshot, error, busy, install, refresh, check, repair } = core;
  const [configuring, setConfiguring] = useState(false);
  const [configuringVideo, setConfiguringVideo] = useState(false);
  const [diagnostics, setDiagnostics] = useState(false);
  const [reinstall, setReinstall] = useState<CoreId | null>(null);
  const [expanded, setExpanded] = useState<CoreId | null>(null);
  if (!snapshot && !error) return <div className="space-y-3"><Skeleton className="h-5 w-24" /><Card className="settings-group"><CardContent className="p-0"><RowsSkeleton label="Carregando Core" count={ACTIVE_CORE_IDS.length} /></CardContent></Card></div>;
  if (!snapshot) return <div role="alert" className="space-y-3"><p className="text-sm text-destructive">{error}</p><Button variant="outline" onClick={() => void refresh()}>Tentar novamente</Button></div>;
  const items = snapshot.items.filter(item => activeCore(item.id));
  const essential = items.filter(item => !optionalCore(item.id));
  const missing = essential.filter(item => !item.bundled && !item.installed).map(item => item.id);
  const reinstallTarget = snapshot.items.find(item => item.id === reinstall);
  return <TooltipProvider delay={150}><section aria-label="Core" className="space-y-4">
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-2"><Cpu className="size-4 text-primary" /><h2 className="text-sm font-medium">Core</h2><Badge variant="outline" className="font-mono text-[10px] text-muted-foreground">{essential.filter(item => item.installed && item.configured && !item.healthError).length}/{essential.length} essenciais</Badge></div>
      <div className="flex items-center gap-1"><Button size="sm" variant="ghost" aria-label="Diagnóstico e Reparo" onClick={() => setDiagnostics(true)}><Wrench className="size-3.5" />Diagnóstico</Button>
<Hint content="Verificar atualizações"><Button size="icon-sm" variant="ghost" aria-label="Verificar atualizações do Core" disabled={snapshot.checking || busy} onClick={() => void check()}><RefreshCw className={`size-3.5 ${snapshot.checking ? "animate-spin" : ""}`} /></Button></Hint></div>
    </div>
    <Card className="settings-group"><CardContent className="divide-y divide-border p-0">{snapshot.browserRuntime && <BrowserRuntimeRow runtime={snapshot.browserRuntime} />}{items.map(item => {
      const { icon: Icon, summary, description } = CORE_DETAILS[item.id];
      const optional = optionalCore(item.id);
      const action = item.installed ? "Atualizar" : "Instalar";
      const usable = item.installed && item.configured && !item.healthError;
      const canInstall = !item.bundled && !item.installed && !item.installedVersion;
      const canReinstall = !item.bundled && Boolean(item.installedVersion && (item.error || item.healthError || !item.installed));
      const open = expanded === item.id;
      return <Collapsible key={item.id} open={open} onOpenChange={next => setExpanded(next ? item.id : null)} className="min-w-0 px-4 py-3">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          <Icon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0 flex-1 basis-48">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5"><h3 className="text-sm font-medium">{item.name}</h3>{item.bundled && <Badge variant="outline" className="text-[10px]">Nativo</Badge>}{item.installedVersion && <span className="font-mono text-[10px] text-muted-foreground">v{item.installedVersion}</span>}{!item.bundled && item.updateAvailable && <span className="font-mono text-[10px] text-primary">→ {item.latestVersion}</span>}</div>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{summary}</p>
            {item.bundled && <p className="text-xs leading-5 text-muted-foreground">Incluído no Jarvis; atualizado com o aplicativo.</p>}
          </div>
          <div className="ml-auto flex flex-wrap items-center gap-1">
            {usable && !item.error ? <span className="px-2 text-xs text-muted-foreground">Pronto</span> : <Badge variant="outline" className="text-[10px] text-onedark-yellow">{item.error ? "Atenção" : item.healthError ? "Reparar" : optional ? "Opcional" : item.installed ? "Configurar chave" : item.installedVersion ? "Reparar" : "Pendente"}</Badge>}
            {!item.bundled && (canInstall || item.updateAvailable) && <Button size="sm" variant={setup ? "default" : "outline"} disabled={busy} onClick={() => void install([item.id])} aria-label={`${action} ${item.name}`} className="cursor-pointer text-xs"><Download className="size-3" />{action}</Button>}
            {canReinstall && <Button size="sm" variant="outline" disabled={busy} onClick={() => setReinstall(item.id)} aria-label={`Reinstalar ${item.name}`} className="cursor-pointer text-xs"><Wrench className="size-3" />Reinstalar</Button>}
            {item.id === "context7" && item.installed && <Hint content={item.configured ? "Alterar chave" : "Configurar Context7"}><Button size={item.configured ? "icon-sm" : "sm"} variant={item.configured ? "ghost" : "outline"} aria-label={item.configured ? "Alterar chave" : "Configurar Context7"} disabled={busy} onClick={() => setConfiguring(true)} className="cursor-pointer text-xs"><KeyRound className="size-3.5" />{!item.configured && "Configurar Context7"}</Button></Hint>}
            {item.id === "openmontage" && item.installed && <Hint content="Configurar OpenMontage"><Button size="icon-sm" variant="ghost" aria-label="Configurar OpenMontage" disabled={busy} onClick={() => setConfiguringVideo(true)} className="cursor-pointer"><KeyRound className="size-3.5" /></Button></Hint>}
            <CollapsibleTrigger render={<Button size="icon-sm" variant="ghost" />} aria-label={`Detalhes de ${item.name}`} className="cursor-pointer text-muted-foreground"><ChevronDown aria-hidden="true" className={`size-3.5 ${open ? "rotate-180" : ""}`} /></CollapsibleTrigger>
          </div>
        </div>
        {item.stage && <CoreInstallProgress item={item} onCancel={() => void core.cancel(item.id)} cancelling={core.cancellingId === item.id} />}
        {(item.healthError || item.error) && !item.stage && <p role="alert" className="mt-2 text-xs text-destructive">{item.healthError ?? item.error}</p>}
        {open && <CollapsibleContent className="ml-7 space-y-2 pt-3">
          <p className="max-w-prose text-xs leading-5 text-muted-foreground">{description}</p>
          {item.id === "impeccable" && !item.bundled && <p className="text-xs leading-5 text-muted-foreground">O Impeccable instala o motor local e as skills de design. O download e a preparação podem levar alguns minutos.</p>}
          <Button size="sm" variant="link" aria-label={`Abrir ${item.name} no GitHub`} className="h-auto cursor-pointer px-0 text-xs" onClick={() => void openUrl(item.repository).catch(() => toast.error("Não foi possível abrir o GitHub."))}><ExternalLink className="size-3" />GitHub</Button>
        </CollapsibleContent>}
      </Collapsible>;
    })}</CardContent></Card>
    {setup && missing.length > 1 && <Button className="w-full" disabled={busy} onClick={() => void install(missing)}><Download className="size-4" />Instalar ferramentas essenciais</Button>}
    {diagnostics && <Suspense fallback={<Skeleton className="h-12 w-full" />}><CoreDiagnostics core={core} onClose={() => setDiagnostics(false)} /></Suspense>}
    <Context7Configuration open={configuring} onOpenChange={setConfiguring} onSaved={refresh} />
    <OpenMontageConfiguration open={configuringVideo} onOpenChange={setConfiguringVideo} onSaved={refresh} />
    <CoreReinstallDialog name={reinstallTarget?.name} open={reinstall !== null} busy={busy} onOpenChange={open => { if (!open) setReinstall(null); }} onConfirm={() => { if (reinstall) void repair(reinstall, true); setReinstall(null); }} />
  </section></TooltipProvider>;
}
const CoreDiagnostics = lazy(() => import("@/components/core/CoreDiagnostics"));
export function CoreSettings() { const core = useCore(); return <CorePanel core={core} />; }

export function Context7Configuration({ open, onOpenChange, onSaved }: { open: boolean; onOpenChange: (open: boolean) => void; onSaved: () => Promise<void> }) {
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  async function save() {
    if (inFlight.current || !key.trim()) return;
    inFlight.current = true; setSaving(true); setError(null);
    try { await invoke("configure_context7", { apiKey: key.trim() }); setKey(""); await onSaved(); onOpenChange(false); toast.success("Context7 configurado"); }
    catch (cause) { setError(coreError(cause)); }
    finally { inFlight.current = false; setSaving(false); }
  }
  return <Dialog open={open} onOpenChange={next => { if (!saving) { setKey(""); setError(null); onOpenChange(next); } }}><DialogContent className="dark sm:max-w-md"><DialogHeader><DialogTitle>Configurar Context7</DialogTitle><DialogDescription>A chave será validada e salva no cofre de credenciais do sistema.</DialogDescription></DialogHeader><form onSubmit={event => { event.preventDefault(); void save(); }} className="space-y-4"><div className="space-y-2"><Label htmlFor="context7-key">Chave de API</Label><Input id="context7-key" type="password" autoComplete="off" value={key} onChange={event => setKey(event.target.value)} disabled={saving} placeholder="ctx7sk-…" /></div><Button type="button" variant="link" className="h-auto p-0 text-xs" onClick={() => void openUrl("https://context7.com/dashboard").catch(() => toast.error("Não foi possível abrir o Context7."))}>Obter chave no Context7<ExternalLink className="size-3" /></Button>{error && <p role="alert" className="text-xs text-destructive">{error}</p>}{saving && <Progress value={null} aria-label="Validando Context7" className="core-install-progress" />}<DialogFooter><Button type="submit" disabled={saving || !key.trim()}>{saving ? "Validando…" : "Salvar e verificar"}</Button></DialogFooter></form></DialogContent></Dialog>;
}
