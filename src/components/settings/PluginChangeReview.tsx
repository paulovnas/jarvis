import { AlertTriangle, AppWindow, BookOpen, Check, Plug, Anchor } from "lucide-react";
import { PLUGIN_COMPONENT_LABELS, type PluginPreview } from "@/core/plugins";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

const ICONS = { skills: BookOpen, mcp: Plug, hooks: Anchor, apps: AppWindow };

/** Shared exact preview for Settings and native agent authorization. */
export function PluginChangeReview({ preview }: { preview: PluginPreview }) {
  return <div className="flex min-w-0 flex-col gap-4">
    <p className="text-sm text-muted-foreground">{preview.description}</p>
    <dl className="grid min-w-0 gap-1 text-xs"><dt className="micro-label text-muted-foreground">Origem</dt><dd className="font-mono wrap-anywhere">{preview.source}</dd>{preview.hash && <><dt className="micro-label mt-2 text-muted-foreground">Versão verificada</dt><dd className="font-mono wrap-anywhere">{preview.hash}</dd></>}</dl>
    {preview.warnings.length > 0 && <Alert><AlertTriangle /><AlertTitle>Atenção antes de continuar</AlertTitle><AlertDescription><ul className="flex list-disc flex-col gap-1 pl-4">{preview.warnings.map((warning, index) => <li key={`${index}:${warning}`}>{warning}</li>)}</ul></AlertDescription></Alert>}
    {preview.requirements.length > 0 && <section aria-label="Requisitos do plugin" className="flex flex-col gap-2"><h3 className="micro-label text-muted-foreground">Requisitos</h3><ul className="flex list-disc flex-col gap-1 pl-4 text-sm text-muted-foreground">{preview.requirements.map((requirement, index) => <li key={`${index}:${requirement}`}>{requirement}</li>)}</ul></section>}
    {preview.components.length > 0 && <section aria-label="Componentes do plugin" className="flex flex-col gap-2"><h3 className="micro-label text-muted-foreground">Componentes</h3>{preview.components.map(component => {
      const Icon = ICONS[component.kind];
      return <Card key={component.id} size="sm" className="gap-2 py-3"><CardHeader className="flex flex-row flex-wrap items-center gap-2"><Icon aria-hidden="true" className="size-4 text-primary" /><CardTitle className="min-w-0 flex-1 break-words">{component.name}</CardTitle><Badge variant="outline">{PLUGIN_COMPONENT_LABELS[component.kind]}</Badge><Badge variant="secondary">{!component.supported ? "Indisponível" : !component.enabled ? "Desativado" : component.kind === "hooks" && !component.trusted ? "Aguardando autorização" : "Habilitado"}</Badge></CardHeader><CardContent><p className="text-xs whitespace-pre-wrap wrap-anywhere text-muted-foreground">{component.detail}</p></CardContent></Card>;
    })}</section>}
    {preview.commands.length > 0 && <section aria-label="Comandos a autorizar" className="flex min-w-0 flex-col gap-2"><h3 className="micro-label text-muted-foreground">Comandos locais</h3><p className="text-xs text-muted-foreground">Confira os comandos completos. Autorizar hooks permite sua execução automática nos eventos das conversas.</p>{preview.commands.map((command, index) => <pre key={`${index}:${command}`} className="rounded-md border bg-muted/30 p-3 font-mono text-xs whitespace-pre-wrap wrap-anywhere">{command}</pre>)}</section>}
    {!preview.components.length && !preview.requirements.length && !preview.commands.length && !preview.warnings.length && <p className="flex items-center gap-2 text-xs text-muted-foreground"><Check aria-hidden="true" className="size-4" />Confira a origem e confirme a alteração.</p>}
  </div>;
}
