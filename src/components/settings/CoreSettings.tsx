import { useEffect, useRef } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { BrainCircuit, Check, Cpu, Download, ExternalLink, GitBranch, RefreshCw, Zap } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { useCore, type CoreController } from "@/hooks/use-core";

const DETAILS = {
  "context-mode": { icon: BrainCircuit, description: "Busca e memória de contexto", color: "text-primary" },
  ponytail: { icon: Zap, description: "Precisão e economia de tokens", color: "text-onedark-yellow" },
  beads: { icon: GitBranch, description: "Planejamento persistente", color: "text-onedark-cyan" },
};

export function CorePanel({ core, setup = false }: { core: CoreController; setup?: boolean }) {
  const { snapshot, error, busy, install, refresh, check } = core;
  const checked = useRef(false);
  useEffect(() => { if (snapshot && !checked.current) { checked.current = true; void check(); } }, [snapshot, check]);
  if (!snapshot && !error) return <div role="status" aria-label="Carregando Core" className="space-y-3"><Skeleton className="mb-5 h-5 w-24" />{[0, 1, 2].map(id => <Skeleton key={id} className="h-24 w-full rounded-lg" />)}</div>;
  if (!snapshot) return <div role="alert" className="space-y-3"><p className="text-sm text-destructive">{error}</p><Button variant="outline" onClick={() => void refresh()}>Tentar novamente</Button></div>;
  const missing = snapshot.items.filter(item => !item.installed).map(item => item.id);
  return <section aria-label="Core" className="space-y-4">
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-2"><Cpu className="size-4 text-primary" /><h2 className="text-sm font-medium">Core</h2><Badge variant="outline" className="font-mono text-[10px] text-muted-foreground">{snapshot.items.filter(item => item.installed).length}/3</Badge></div>
      <Button size="icon-sm" variant="ghost" aria-label="Verificar atualizações do Core" title="Verificar atualizações" disabled={snapshot.checking || busy} onClick={() => void check()}><RefreshCw className={`size-3.5 ${snapshot.checking ? "animate-spin" : ""}`} /></Button>
    </div>
    {setup && <p className="text-xs text-muted-foreground">Instale os três componentes para continuar.</p>}
    <div className="space-y-2">{snapshot.items.map(item => {
      const { icon: Icon, description, color } = DETAILS[item.id];
      const action = item.installed ? "Atualizar" : item.installedVersion ? "Reinstalar" : "Instalar";
      return <Card key={item.id} className="instrument-panel gap-0 py-0">
        <CardContent className="p-4">
          <div className="flex flex-wrap items-center gap-3">
            <div className={`flex size-9 shrink-0 items-center justify-center rounded-md border border-border bg-background/60 ${color}`}><Icon className="size-4" /></div>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2"><h3 className="text-xs font-medium">{item.name}</h3>{item.installed && <Badge variant="outline" className="gap-1 border-onedark-green/25 bg-onedark-green/10 text-[9px] text-onedark-green"><Check className="size-2.5" />Instalado</Badge>}</div>
              <p className="mt-1 text-[11px] text-muted-foreground">{description}</p>
            </div>
            <div className="flex items-center gap-2">
              {item.installedVersion && <span className="font-mono text-[10px] text-muted-foreground">v{item.installedVersion}</span>}
              {item.updateAvailable && <span className="font-mono text-[10px] text-primary">→ {item.latestVersion}</span>}
              {(!item.installed || item.updateAvailable) && <Button size="sm" variant={setup ? "default" : "outline"} disabled={busy} onClick={() => void install([item.id])} aria-label={`${action} ${item.name}`} className="text-xs"><Download className="size-3" />{action}</Button>}
              <Button size="icon-sm" variant="ghost" aria-label={`Abrir ${item.name} no GitHub`} onClick={() => void openUrl(item.repository).catch(() => toast.error("Não foi possível abrir o GitHub."))}><ExternalLink className="size-3 text-muted-foreground" /></Button>
            </div>
          </div>
          {item.stage && <div role="status" className="mt-3 flex items-center gap-2 border-t border-border pt-3 text-[11px] text-primary"><Spinner className="size-3" />{item.stage}</div>}
          {item.error && !item.stage && <p role="alert" className="mt-3 border-t border-border pt-3 text-xs text-destructive">{item.error}</p>}
        </CardContent>
      </Card>;
    })}</div>
    {setup && missing.length > 1 && <Button className="w-full" disabled={busy} onClick={() => void install(missing)}><Download className="size-4" />Instalar Core</Button>}
    <p className="font-mono text-[10px] text-muted-foreground/70">~/.jarvis/core</p>
  </section>;
}
export function CoreSettings() { const core = useCore(); return <CorePanel core={core} />; }
