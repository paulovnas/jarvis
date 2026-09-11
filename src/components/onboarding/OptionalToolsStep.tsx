import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, Download, ExternalLink, GitBranch, GitPullRequest, RefreshCw, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import {
  optionalToolsError,
  optionalToolsSnapshotSchema,
  type OptionalTool,
  type OptionalToolId,
  type OptionalToolsSnapshot,
} from "@/core/optional-tools";

const ICONS = { git: GitBranch, gh: GitPullRequest } as const;

function ToolsSkeleton() {
  return <div role="status" aria-label="Verificando ferramentas opcionais" className="space-y-5">
    <div className="space-y-2"><Skeleton className="h-8 w-64" /><Skeleton className="h-4 w-full max-w-xl" /></div>
    <div className="grid gap-4 sm:grid-cols-2"><Skeleton className="h-56" /><Skeleton className="h-56" /></div>
  </div>;
}

function ToolCard({ tool, installing, disabled, onInstall, onHelp }: {
  tool: OptionalTool;
  installing: boolean;
  disabled: boolean;
  onInstall: (id: OptionalToolId) => void;
  onHelp: (url: string) => void;
}) {
  const Icon = ICONS[tool.id];
  return <Card className={tool.installed ? "border-onedark-green/25" : "border-border"}>
    <CardHeader className="border-b border-border">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-center gap-3">
          <div className={`flex size-9 shrink-0 items-center justify-center rounded-md border ${tool.installed ? "border-onedark-green/25 bg-onedark-green/10 text-onedark-green" : "border-primary/20 bg-primary/10 text-primary"}`}><Icon className="size-4" /></div>
          <div className="space-y-1"><CardTitle className="text-sm">{tool.name}</CardTitle><Badge variant="outline" className={tool.installed ? "text-onedark-green" : "text-onedark-yellow"}>{tool.installed ? "Instalado" : "Opcional"}</Badge></div>
        </div>
        {tool.installed && <CheckCircle2 aria-label={`${tool.name} disponível`} className="size-5 text-onedark-green" />}
      </div>
      <CardDescription className="pt-2 leading-5">{tool.description}</CardDescription>
    </CardHeader>
    <CardContent className="min-h-16 pt-5">
      {tool.version
        ? <p className="break-words font-mono text-[11px] text-muted-foreground">{tool.version}</p>
        : <p className="text-xs leading-5 text-muted-foreground">Você pode continuar sem esta ferramenta e instalá-la depois.</p>}
    </CardContent>
    {!tool.installed && <CardFooter className="flex flex-wrap gap-2 border-t border-border pt-4">
      {tool.automaticInstall && tool.installWith
        ? <Button className="cursor-pointer gap-2" aria-label={`Instalar ${tool.name} com ${tool.installWith}`} disabled={disabled} aria-busy={installing} onClick={() => onInstall(tool.id)}>{installing ? <Spinner /> : <Download className="size-4" />}{installing ? "Instalando…" : `Instalar com ${tool.installWith}`}</Button>
        : <Button variant="outline" className="cursor-pointer gap-2" aria-label={`Abrir instruções de ${tool.name}`} disabled={disabled} onClick={() => onHelp(tool.helpUrl)}>Abrir instruções<ExternalLink className="size-4" /></Button>}
      {tool.automaticInstall && <Button variant="ghost" size="sm" className="cursor-pointer gap-1.5" aria-label={`Abrir opções manuais de ${tool.name}`} disabled={disabled} onClick={() => onHelp(tool.helpUrl)}>Opções manuais<ExternalLink className="size-3" /></Button>}
    </CardFooter>}
  </Card>;
}

export function OptionalToolsStep({ onBusyChange }: { onBusyChange: (busy: boolean) => void }) {
  const [snapshot, setSnapshot] = useState<OptionalToolsSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState<OptionalToolId | null>(null);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const busy = loading || installing !== null;

  useEffect(() => { onBusyChange(busy); }, [busy, onBusyChange]);

  const load = useCallback((request: number) => {
    return invoke<unknown>("get_optional_tools_status").then(value => {
      const next = optionalToolsSnapshotSchema.parse(value);
      if (generation.current === request) { setSnapshot(next); setError(null); }
    }).catch(cause => {
      if (generation.current === request) setError(optionalToolsError(cause, "Não foi possível verificar Git e GitHub CLI."));
    }).finally(() => {
      if (generation.current === request) setLoading(false);
    });
  }, []);

  useEffect(() => {
    const request = ++generation.current;
    void load(request);
    return () => { generation.current += 1; };
  }, [load]);

  const install = async (id: OptionalToolId) => {
    if (busy) return;
    setInstalling(id); setError(null);
    try {
      const next = optionalToolsSnapshotSchema.parse(await invoke<unknown>("install_optional_tool", { id }));
      setSnapshot(next);
      const tool = next.tools.find(item => item.id === id);
      toast.success(`${tool?.name ?? "Ferramenta"} instalado`);
    } catch (cause) {
      const message = optionalToolsError(cause, "Não foi possível instalar a ferramenta.");
      setError(message); toast.error(message);
    } finally { setInstalling(null); }
  };

  const openHelp = (url: string) => {
    void openUrl(url).catch(() => toast.error("Não foi possível abrir as instruções de instalação."));
  };

  if (loading && !snapshot) return <ToolsSkeleton />;
  return <div className="space-y-5">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="space-y-2"><h1 className="text-2xl font-medium tracking-tight">Complete seu ambiente</h1><p className="max-w-2xl text-sm leading-6 text-muted-foreground">Git e GitHub CLI ampliam os recursos de versionamento e publicação. Ambos são opcionais e podem ser instalados depois.</p></div>
      {snapshot && <Badge variant="outline" className="font-mono text-[10px]">{snapshot.platformLabel}</Badge>}
    </div>
    {error && <Alert className="border-onedark-yellow/25 bg-onedark-yellow/5"><TriangleAlert className="text-onedark-yellow" /><AlertTitle>Verificação incompleta</AlertTitle><AlertDescription>{error} Você ainda pode avançar.</AlertDescription></Alert>}
    {snapshot && <div className="grid gap-4 sm:grid-cols-2">{snapshot.tools.map(tool => <ToolCard key={tool.id} tool={tool} installing={installing === tool.id} disabled={busy} onInstall={id => { void install(id); }} onHelp={openHelp} />)}</div>}
    <div className="flex items-center justify-between gap-4 border-t border-border pt-4">
      <p className="text-[11px] leading-5 text-muted-foreground">A instalação automática usa apenas o gerenciador detectado no sistema.</p>
      <Button variant="ghost" size="sm" className="shrink-0 cursor-pointer gap-2" disabled={busy} onClick={() => { setLoading(true); const request = ++generation.current; void load(request); }}><RefreshCw className={`size-3.5 ${loading ? "animate-spin" : ""}`} />Verificar novamente</Button>
    </div>
  </div>;
}
