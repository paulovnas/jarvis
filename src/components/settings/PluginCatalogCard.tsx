import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { PLUGIN_COMPONENT_LABELS, type AvailablePlugin, type InstalledPlugin } from "@/core/plugins";
import { PluginIcon } from "./PluginIcon";

export function PluginCatalogCard({ plugin, installed, available, marketplaceName, busy, onDetails, onInstall, onEnabled }: {
  plugin: AvailablePlugin | InstalledPlugin;
  installed?: InstalledPlugin;
  available?: AvailablePlugin;
  marketplaceName?: string;
  busy: boolean;
  onDetails: () => void;
  onInstall: () => void;
  onEnabled: (enabled: boolean) => void;
}) {
  return <Card size="sm" className="min-w-0">
    <CardHeader className="flex flex-row items-start gap-3">
      <PluginIcon name={plugin.displayName} src={plugin.iconDataUrl ?? installed?.iconDataUrl} />
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-2"><CardTitle className="min-w-0 break-words">{plugin.displayName}</CardTitle>{plugin.version && <Badge variant="outline" className="font-mono">{plugin.version}</Badge>}</div>
        <CardDescription className="line-clamp-3 text-xs">{plugin.shortDescription || plugin.description || "Pacote de recursos para os agentes."}</CardDescription>
      </div>
    </CardHeader>
    <CardContent className="flex flex-col gap-2">
      <div className="flex flex-wrap gap-1.5">{installed ? <>
        <Badge variant={installed.enabled && installed.integrityValid ? "secondary" : "outline"}>{!installed.integrityValid ? "Revisão necessária" : installed.enabled ? "Ativo" : "Desativado"}</Badge>
        {[...new Set(installed.components.map(component => component.kind))].map(kind => <Badge key={kind} variant="outline">{PLUGIN_COMPONENT_LABELS[kind]}</Badge>)}
      </> : <Badge variant="outline">{available && !available.installable ? "Requisito pendente" : "Disponível"}</Badge>}</div>
      {marketplaceName && <p className="truncate text-xs text-muted-foreground" title={marketplaceName}>{marketplaceName}</p>}
    </CardContent>
    <CardFooter className="mt-auto flex flex-wrap justify-between gap-2">
      <Button variant="ghost" size="sm" className="cursor-pointer" aria-label={`Detalhes de ${plugin.displayName}`} disabled={busy} onClick={onDetails}>Detalhes</Button>
      {installed ? <Switch className="cursor-pointer" aria-label={`Ativar plugin ${plugin.displayName}`} checked={installed.enabled && installed.integrityValid} disabled={busy || !installed.integrityValid} onCheckedChange={onEnabled} /> : <Button size="sm" className="cursor-pointer" aria-label={`Instalar ${plugin.displayName}`} disabled={busy || !available?.installable} onClick={onInstall}>Instalar</Button>}
    </CardFooter>
  </Card>;
}
