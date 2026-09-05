import { useId } from "react";
import { ChevronRight, Pencil, Plug, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Skeleton } from "@/components/ui/skeleton";
import type { McpServer } from "@/core/mcp";

export function McpServerCard({ server, busy, checking = false, onToggle, onEdit, onDelete, onTest }: {
  server: McpServer; busy: boolean; checking?: boolean; onToggle: (enabled: boolean) => void; onEdit: () => void; onDelete: () => void; onTest: () => void;
}) {
  const id = useId();
  const status = !server.enabled ? "Desativado" : !server.configured ? "Configuração pendente" : server.lastCheck?.error ? "Verificar conexão" : "Ativado";
  const statusColor = !server.enabled ? "text-muted-foreground" : !server.configured ? "border-[#e5c07b]/30 bg-[#e5c07b]/10 text-[#e5c07b]" : server.lastCheck?.error ? "border-destructive/30 bg-destructive/10 text-destructive" : "border-[#98c379]/30 bg-[#98c379]/10 text-[#98c379]";
  return <Collapsible render={<Card size="sm" />} className="gap-0 py-0">
    <CollapsibleTrigger render={<CardHeader />} nativeButton={false} aria-label={`Detalhes do MCP ${server.name}`} className="group cursor-pointer rounded-xl py-3 transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset">
      <div className="flex min-w-0 items-center gap-3">
        <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"><Plug aria-hidden="true" className="size-4" /></div>
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2"><CardTitle className="truncate">{server.name}</CardTitle><Badge variant="outline" className={statusColor}>{status}</Badge></div>
          <CardDescription>{server.kind === "local" ? "Local · stdio" : "Remoto · HTTP"}{server.lastCheck && !server.lastCheck.error && ` · ${server.lastCheck.toolCount} ferramentas`}</CardDescription>
        </div>
        <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </div>
    </CollapsibleTrigger>
    <CollapsibleContent className="flex flex-col gap-4 border-t border-border/70 pt-4">
      <CardContent className="flex flex-col gap-2 text-xs text-muted-foreground">
        {!server.configured && <p>Adicione sua chave de API.</p>}
        {checking && <div role="status" aria-label="Descobrindo ferramentas" className="flex flex-wrap gap-1.5"><Skeleton className="h-5 w-28 rounded-full" /><Skeleton className="h-5 w-36 rounded-full" /><Skeleton className="h-5 w-24 rounded-full" /></div>}
        {!!server.lastCheck?.tools.length && <div aria-label={`Ferramentas de ${server.name}`} className="flex max-h-56 flex-wrap gap-1.5 overflow-y-auto">{server.lastCheck.tools.map(tool => <Badge key={tool} variant="outline" className="max-w-full break-all whitespace-normal border-[#56b6c2]/30 bg-[#56b6c2]/10 text-[#56b6c2]">{tool}</Badge>)}</div>}
        {server.lastCheck && !server.lastCheck.error && server.lastCheck.tools.length === 0 && !checking && <p>Nenhuma ferramenta encontrada.</p>}
        {server.lastCheck?.error && <p role="alert" className="text-destructive">{server.lastCheck.error}</p>}
      </CardContent>
      <CardFooter className="flex flex-wrap justify-between gap-3">
        <Label htmlFor={id} className="cursor-pointer gap-2 text-xs"><Switch id={id} checked={server.enabled} onCheckedChange={onToggle} disabled={busy} className="cursor-pointer" /><span className="sr-only">Ativar MCP {server.name}</span><span aria-hidden="true">{server.enabled ? "Ativado" : "Desativado"}</span></Label>
        <div className="flex flex-wrap gap-1">
          <Button variant="outline" size="sm" className="cursor-pointer" onClick={onTest} disabled={busy || !server.enabled || !server.configured}>Testar conexão</Button>
          <Button variant="ghost" size="sm" className="cursor-pointer" onClick={onEdit} disabled={busy}><Pencil aria-hidden="true" />Editar</Button>
          <Button variant="ghost" size="sm" className="cursor-pointer text-destructive" onClick={onDelete} disabled={busy}><Trash2 aria-hidden="true" />Excluir</Button>
        </div>
      </CardFooter>
    </CollapsibleContent>
  </Collapsible>;
}
