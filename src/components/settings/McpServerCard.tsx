import { useId, useState } from "react";
import { Blocks, ChevronRight, LockKeyhole, Pencil, Plug, Trash2, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { Label } from "@/components/ui/label";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { Switch } from "@/components/ui/switch";
import { Skeleton } from "@/components/ui/skeleton";
import type { McpServer } from "@/core/mcp";
import { PluginMcpAuth } from "./PluginMcpAuth";

export function McpServerCard({ server, busy, checking = false, onToggle, onEdit, onDelete, onTest }: {
  server: McpServer; busy: boolean; checking?: boolean; onToggle: (enabled: boolean) => void; onEdit: () => void; onDelete: () => void; onTest: () => void;
}) {
  const id = useId();
  const [authBusy, setAuthBusy] = useState(false);
  const disabled = busy || authBusy;
  const status = !server.enabled ? "Desativado" : !server.configured ? "Configuração pendente" : server.lastCheck?.error ? "Verificar conexão" : "Ativado";
  const statusColor = !server.enabled ? "text-muted-foreground" : !server.configured ? "border-onedark-yellow/30 bg-onedark-yellow/10 text-onedark-yellow" : server.lastCheck?.error ? "border-destructive/30 bg-destructive/10 text-destructive" : "border-onedark-green/30 bg-onedark-green/10 text-onedark-green";
  return <Sheet>
    <Card size="sm" className="settings-group min-w-0">
      <SheetTrigger render={<Button variant="ghost" />} aria-label={`Detalhes do MCP ${server.name}`} className="settings-row h-auto w-full cursor-pointer justify-start gap-3 rounded-[inherit] p-4 text-left whitespace-normal hover:bg-accent/50">
        <Plug aria-hidden="true" className="size-5 shrink-0 text-muted-foreground" />
        <span className="flex min-w-0 flex-1 flex-col gap-1.5">
          <Hint content={server.name} whenTruncated><span className="truncate text-sm font-medium">{server.name}</span></Hint>
          <span className="flex flex-wrap items-center gap-1.5">
            <Badge variant="outline" className={statusColor}>{status}</Badge>
            {server.source && <Hint content={`Plugin: ${server.source}`}><Badge variant="outline" className="max-w-full gap-1 text-muted-foreground"><Blocks aria-hidden="true" className="size-3 shrink-0" /><span className="truncate">Plugin · {server.source}</span></Badge></Hint>}
          </span>
          <span className="text-xs font-normal text-muted-foreground">{server.kind === "local" ? "Local · stdio" : "Remoto · HTTP"}{server.lastCheck && !server.lastCheck.error && ` · ${server.lastCheck.toolCount} ferramentas`}</span>
        </span>
        <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
      </SheetTrigger>
    </Card>
    <SheetContent showCloseButton={false} className="dark gap-0 overflow-hidden data-[side=right]:w-full data-[side=right]:sm:max-w-xl motion-reduce:transition-none">
      <SheetHeader className="shrink-0 gap-2 border-b px-5 py-5 pr-14">
        <SheetTitle className="break-words">{server.name}</SheetTitle>
        <SheetDescription>Ferramentas e conexão deste MCP.</SheetDescription>
        <div className="flex flex-wrap gap-2"><Badge variant="outline" className={statusColor}>{status}</Badge>{server.source && <Badge variant="outline" className="gap-1 text-primary"><LockKeyhole aria-hidden="true" className="size-3" />Gerenciado em Plugins</Badge>}</div>
      </SheetHeader>
      <SheetClose render={<Button variant="ghost" size="icon-sm" />} aria-label="Fechar detalhes do MCP" className="absolute top-4 right-4 cursor-pointer"><X aria-hidden="true" /></SheetClose>
      <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 py-5">
        {server.source && <div className="flex min-w-0 flex-col gap-2 text-xs"><Badge variant="outline" className="max-w-full self-start gap-1"><Blocks aria-hidden="true" className="size-3 shrink-0" /><span className="break-all whitespace-normal">Plugin · {server.source}</span></Badge><p className="text-muted-foreground">Para remover este MCP, remova o plugin na seção Plugins. Suas configurações também são gerenciadas lá.</p></div>}
        {!server.configured && <p className="text-sm text-muted-foreground">{server.source ? "Conclua a configuração na seção Plugins." : "Adicione sua chave de API."}</p>}
        {server.kind === "remote" && server.enabled && server.configured && <PluginMcpAuth key={server.revision} serverId={server.id} name={server.name} disabled={busy} onBusyChange={setAuthBusy} onStatusChange={onTest} />}
        <section aria-label={`Ferramentas de ${server.name}`} className="flex min-w-0 flex-col gap-3">
          <h3 className="text-sm font-medium">Ferramentas{server.lastCheck && !server.lastCheck.error && ` (${server.lastCheck.toolCount})`}</h3>
          {checking && <div role="status" aria-label="Descobrindo ferramentas" className="grid gap-2 sm:grid-cols-2"><Skeleton className="h-7 bg-secondary" /><Skeleton className="h-7 bg-secondary" /><Skeleton className="h-7 bg-secondary" /></div>}
          {!!server.lastCheck?.tools.length && <ul className="grid min-w-0 gap-2 sm:grid-cols-2">{server.lastCheck.tools.map(tool => <li key={tool} className="min-w-0"><Badge variant="outline" className="w-full justify-start rounded-md px-2 py-1.5 font-mono text-[11px] break-all whitespace-normal text-muted-foreground">{tool}</Badge></li>)}</ul>}
          {server.lastCheck && !server.lastCheck.error && server.lastCheck.tools.length === 0 && !checking && <p className="text-xs text-muted-foreground">Nenhuma ferramenta encontrada.</p>}
          {!server.lastCheck && !checking && server.configured && <p className="text-xs text-muted-foreground">Teste a conexão para descobrir as ferramentas disponíveis.</p>}
          {server.lastCheck?.error && <p role="alert" className="text-xs text-destructive">{server.lastCheck.error}</p>}
        </section>
      </div>
      <SheetFooter className="shrink-0 gap-4 border-t px-5 py-4">
        {!server.source && <Label htmlFor={id} className="cursor-pointer gap-2 text-xs"><Switch id={id} checked={server.enabled} onCheckedChange={onToggle} disabled={disabled} className="cursor-pointer" /><span className="sr-only">Ativar MCP {server.name}</span><span aria-hidden="true">{server.enabled ? "Ativado" : "Desativado"}</span></Label>}
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" className="cursor-pointer" onClick={onTest} disabled={disabled || !server.enabled || !server.configured}>Testar conexão</Button>
          {!server.source && <><Button variant="ghost" size="sm" className="cursor-pointer" onClick={onEdit} disabled={disabled}><Pencil aria-hidden="true" />Editar</Button><Button variant="ghost" size="sm" className="cursor-pointer text-destructive" onClick={onDelete} disabled={disabled}><Trash2 aria-hidden="true" />Excluir</Button></>}
        </div>
      </SheetFooter>
    </SheetContent>
  </Sheet>;
}
