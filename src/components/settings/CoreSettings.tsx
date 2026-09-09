import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, CircleHelp, Cpu, Download, ExternalLink, KeyRound, RefreshCw, Wrench } from "lucide-react";
import { CORE_DETAILS } from "@/core/core-presentation";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Progress } from "@/components/ui/progress";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { coreError } from "@/core/core-components";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Input } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { useCore, type CoreController } from "@/hooks/use-core";
import { CoreInstallProgress } from "@/components/core/CoreInstallProgress";

export function CorePanel({ core, setup = false }: { core: CoreController; setup?: boolean }) {
  const { snapshot, error, busy, install, refresh, check } = core;
  const [configuring, setConfiguring] = useState(false);
  const [diagnostics, setDiagnostics] = useState(false);
  const checkStarted = useRef(false);
  useEffect(() => { if (snapshot && !core.checked && !checkStarted.current) { checkStarted.current = true; void check(); } }, [snapshot, core.checked, check]);
  if (!snapshot && !error) return <div role="status" aria-label="Carregando Core" className="space-y-3"><Skeleton className="mb-5 h-5 w-24" /><div className="core-card-grid">{[0, 1, 2, 3, 4].map(id => <Skeleton key={id} className="h-56 w-full rounded-lg" />)}</div></div>;
  if (!snapshot) return <div role="alert" className="space-y-3"><p className="text-sm text-destructive">{error}</p><Button variant="outline" onClick={() => void refresh()}>Tentar novamente</Button></div>;
  const missing = snapshot.items.filter(item => !item.installed).map(item => item.id);
  return <TooltipProvider delay={150}><section aria-label="Core" className="space-y-4">
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-2"><Cpu className="size-4 text-primary" /><h2 className="text-sm font-medium">Core</h2><Badge variant="outline" className="font-mono text-[10px] text-muted-foreground">{snapshot.items.filter(item => item.installed && item.configured && !item.healthError).length}/{snapshot.items.length}</Badge></div>
      <div className="flex items-center gap-1"><Button size="sm" variant="ghost" aria-label="Diagnóstico e Reparo" onClick={() => setDiagnostics(true)}><Wrench className="size-3.5" />Diagnóstico</Button>
<Button size="icon-sm" variant="ghost" aria-label="Verificar atualizações do Core" title="Verificar atualizações" disabled={snapshot.checking || busy} onClick={() => void check()}><RefreshCw className={`size-3.5 ${snapshot.checking ? "animate-spin" : ""}`} /></Button></div>
    </div>
    <div className="core-card-grid">{snapshot.items.map(item => {
      const { icon: Icon, label, description, color, tint } = CORE_DETAILS[item.id];
      const action = item.installed ? "Atualizar" : item.installedVersion ? "Reinstalar" : "Instalar";
      return <Card key={item.id} className="instrument-panel gap-0 py-0">
        <CardContent className="flex h-full min-w-0 flex-col p-4">
          <div className="mb-4 flex items-start justify-between">
            <div className={`flex size-10 items-center justify-center rounded-md border ${color} ${tint}`}><Icon className="size-5" /></div>
            <div className="flex items-center gap-1">
              {item.id === "open-design" && <Tooltip><TooltipTrigger render={<Button size="icon-sm" variant="ghost" />} aria-label="Sobre a instalação do Open Design" className="cursor-pointer text-muted-foreground"><CircleHelp className="size-3.5" /></TooltipTrigger><TooltipContent className="max-w-64 border border-border bg-card text-xs leading-5 text-foreground">O Open Design inclui muitos recursos visuais. O download e a preparação podem levar alguns minutos.</TooltipContent></Tooltip>}
              <Button size="icon-sm" variant="ghost" aria-label={`Abrir ${item.name} no GitHub`} onClick={() => void openUrl(item.repository).catch(() => toast.error("Não foi possível abrir o GitHub."))}><ExternalLink className="size-3 text-muted-foreground" /></Button>
            </div>
          </div>
          <p className={`micro-label mb-1 ${color}`}>{label}</p>
          <h3 className="text-sm font-medium">{item.name}</h3>
          <p className="mb-4 mt-2 text-xs leading-5 text-muted-foreground">{description}</p>
          <div className="mt-auto space-y-3 border-t border-border/70 pt-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              {item.installed && item.configured && !item.healthError ? <Badge variant="outline" className="gap-1 border-onedark-green/25 bg-onedark-green/10 text-[9px] text-onedark-green"><Check className="size-2.5" />Pronto</Badge> : <Badge variant="outline" className="border-onedark-yellow/25 text-[9px] text-onedark-yellow">{item.healthError ? "Reparar" : item.installed ? "Configurar chave" : item.installedVersion ? "Reparar" : "Pendente"}</Badge>}
              {item.installedVersion && <span className="font-mono text-[10px] text-muted-foreground">v{item.installedVersion}</span>}
            </div>
            {item.updateAvailable && <span className="block font-mono text-[10px] text-primary">→ {item.latestVersion}</span>}
            {(!item.installed || item.updateAvailable) && <Button size="sm" variant={setup ? "default" : "outline"} disabled={busy} onClick={() => void install([item.id])} aria-label={`${action} ${item.name}`} className="w-full text-xs"><Download className="size-3" />{action}</Button>}
            {item.id === "context7" && item.installed && <Button size="sm" variant={item.configured ? "ghost" : "outline"} disabled={busy} onClick={() => setConfiguring(true)} className="w-full text-xs"><KeyRound className="size-3" />{item.configured ? "Alterar chave" : "Configurar Context7"}</Button>}
          </div>
          {item.stage && <CoreInstallProgress item={item} />}
          {(item.healthError || item.error) && !item.stage && <p role="alert" className="mt-3 border-t border-border pt-3 text-xs text-destructive">{item.healthError ?? item.error}</p>}
        </CardContent>
      </Card>;
    })}</div>
    {setup && missing.length > 1 && <Button className="w-full" disabled={busy} onClick={() => void install(missing)}><Download className="size-4" />Instalar Core</Button>}
    {diagnostics && <Suspense fallback={<Skeleton className="h-12 w-full" />}><CoreDiagnostics core={core} onClose={() => setDiagnostics(false)} /></Suspense>}
    <Context7Configuration open={configuring} onOpenChange={setConfiguring} onSaved={refresh} />
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
  return <Dialog open={open} onOpenChange={next => { if (!saving) { setKey(""); setError(null); onOpenChange(next); } }}><DialogContent className="dark sm:max-w-md"><DialogHeader><DialogTitle>Configurar Context7</DialogTitle><DialogDescription>A chave será validada e salva no Keychain.</DialogDescription></DialogHeader><form onSubmit={event => { event.preventDefault(); void save(); }} className="space-y-4"><div className="space-y-2"><Label htmlFor="context7-key">Chave de API</Label><Input id="context7-key" type="password" autoComplete="off" value={key} onChange={event => setKey(event.target.value)} disabled={saving} placeholder="ctx7sk-…" /></div><Button type="button" variant="link" className="h-auto p-0 text-xs" onClick={() => void openUrl("https://context7.com/dashboard").catch(() => toast.error("Não foi possível abrir o Context7."))}>Obter chave no Context7<ExternalLink className="size-3" /></Button>{error && <p role="alert" className="text-xs text-destructive">{error}</p>}{saving && <Progress value={null} aria-label="Validando Context7" className="core-install-progress" />}<DialogFooter><Button type="submit" disabled={saving || !key.trim()}>{saving ? "Validando…" : "Salvar e verificar"}</Button></DialogFooter></form></DialogContent></Dialog>;
}
