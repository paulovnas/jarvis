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
  "context-mode": { icon: BrainCircuit, label: "Memória", description: "Indexa e recupera informações do projeto sob demanda, preservando espaço no contexto da conversa.", color: "text-primary", tint: "border-primary/25 bg-primary/10" },
  ponytail: { icon: Zap, label: "Precisão", description: "Orienta a escrita e a revisão de código com diretrizes de programação, reduzindo ruído e retrabalho.", color: "text-onedark-yellow", tint: "border-onedark-yellow/25 bg-onedark-yellow/10" },
  beads: { icon: GitBranch, label: "Planejamento", description: "Organiza épicos, tarefas e dependências. Mantém o progresso do projeto entre conversas e agentes.", color: "text-onedark-cyan", tint: "border-onedark-cyan/25 bg-onedark-cyan/10" },
};

export function CorePanel({ core, setup = false }: { core: CoreController; setup?: boolean }) {
  const { snapshot, error, busy, install, refresh, check } = core;
  const checked = useRef(false);
  useEffect(() => { if (snapshot && !checked.current) { checked.current = true; void check(); } }, [snapshot, check]);
  if (!snapshot && !error) return <div role="status" aria-label="Carregando Core" className="space-y-3"><Skeleton className="mb-5 h-5 w-24" /><div className="grid gap-3 sm:grid-cols-3">{[0, 1, 2].map(id => <Skeleton key={id} className="h-56 w-full rounded-lg" />)}</div></div>;
  if (!snapshot) return <div role="alert" className="space-y-3"><p className="text-sm text-destructive">{error}</p><Button variant="outline" onClick={() => void refresh()}>Tentar novamente</Button></div>;
  const missing = snapshot.items.filter(item => !item.installed).map(item => item.id);
  return <section aria-label="Core" className="space-y-4">
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-2"><Cpu className="size-4 text-primary" /><h2 className="text-sm font-medium">Core</h2><Badge variant="outline" className="font-mono text-[10px] text-muted-foreground">{snapshot.items.filter(item => item.installed).length}/3</Badge></div>
      <Button size="icon-sm" variant="ghost" aria-label="Verificar atualizações do Core" title="Verificar atualizações" disabled={snapshot.checking || busy} onClick={() => void check()}><RefreshCw className={`size-3.5 ${snapshot.checking ? "animate-spin" : ""}`} /></Button>
    </div>
    {setup && <p className="text-xs text-muted-foreground">Instale os três componentes para continuar.</p>}
    <div className="grid gap-3 sm:grid-cols-3">{snapshot.items.map(item => {
      const { icon: Icon, label, description, color, tint } = DETAILS[item.id];
      const action = item.installed ? "Atualizar" : item.installedVersion ? "Reinstalar" : "Instalar";
      return <Card key={item.id} className="instrument-panel gap-0 py-0">
        <CardContent className="flex h-full min-w-0 flex-col p-4">
          <div className="mb-4 flex items-start justify-between">
            <div className={`flex size-10 items-center justify-center rounded-md border ${color} ${tint}`}><Icon className="size-5" /></div>
            <Button size="icon-sm" variant="ghost" aria-label={`Abrir ${item.name} no GitHub`} onClick={() => void openUrl(item.repository).catch(() => toast.error("Não foi possível abrir o GitHub."))}><ExternalLink className="size-3 text-muted-foreground" /></Button>
          </div>
          <p className={`micro-label mb-1 ${color}`}>{label}</p>
          <h3 className="text-sm font-medium">{item.name}</h3>
          <p className="mb-4 mt-2 text-xs leading-5 text-muted-foreground">{description}</p>
          <div className="mt-auto space-y-3 border-t border-border/70 pt-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              {item.installed ? <Badge variant="outline" className="gap-1 border-onedark-green/25 bg-onedark-green/10 text-[9px] text-onedark-green"><Check className="size-2.5" />Instalado</Badge> : <Badge variant="outline" className="border-onedark-yellow/25 text-[9px] text-onedark-yellow">{item.installedVersion ? "Reparar" : "Pendente"}</Badge>}
              {item.installedVersion && <span className="font-mono text-[10px] text-muted-foreground">v{item.installedVersion}</span>}
            </div>
            {item.updateAvailable && <span className="block font-mono text-[10px] text-primary">→ {item.latestVersion}</span>}
            {(!item.installed || item.updateAvailable) && <Button size="sm" variant={setup ? "default" : "outline"} disabled={busy} onClick={() => void install([item.id])} aria-label={`${action} ${item.name}`} className="w-full text-xs"><Download className="size-3" />{action}</Button>}
          </div>
          {item.stage && <div role="status" className="mt-3 flex items-center gap-2 border-t border-border pt-3 text-[11px] text-primary"><Spinner className="size-3" />{item.stage}</div>}
          {item.error && !item.stage && <p role="alert" className="mt-3 border-t border-border pt-3 text-xs text-destructive">{item.error}</p>}
        </CardContent>
      </Card>;
    })}</div>
    {setup && missing.length > 1 && <Button className="w-full" disabled={busy} onClick={() => void install(missing)}><Download className="size-4" />Instalar Core</Button>}
  </section>;
}
export function CoreSettings() { const core = useCore(); return <CorePanel core={core} />; }
