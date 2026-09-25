import { ChevronRight, Cpu, TriangleAlert } from "lucide-react";
import { useMemo } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import type { CoreActivity } from "@/core/chat";
import type { AssistantWorkData, ToolCallItem } from "./types";

const names: Record<CoreActivity["component"], string> = {
  "context-mode": "Context-mode", ponytail: "Ponytail", beads: "Beads",
  "open-design": "Open Design", context7: "Context7", lsp: "LSP",
};
const statuses = { applied: "Aplicado", reused: "Reutilizado", unavailable: "Indisponível", pending: "Aguardando diagnóstico", issues: "Diagnósticos encontrados" };

function componentFor(tool: ToolCallItem): CoreActivity["component"] | undefined {
  if (tool.name.startsWith("ctx_")) return "context-mode";
  if (tool.name.startsWith("design_")) return "open-design";
  if (tool.name.startsWith("context7_")) return "context7";
  if (tool.name.startsWith("beads_")) return "beads";
  if (tool.name.startsWith("lsp_")) return "lsp";
}

type DiagnosticState = { current: CoreActivity; warning?: CoreActivity; resolved?: CoreActivity };
type Resource = { id: CoreActivity["component"]; receipts: CoreActivity[]; tools: ToolCallItem[]; diagnostics: Map<string, DiagnosticState> };

function hasWarning(status: CoreActivity["status"]) {
  return status === "unavailable" || status === "pending" || status === "issues";
}

export function CoreActivitySummary({ steps }: { steps: AssistantWorkData["steps"] }) {
  const resources = useMemo(() => {
    const grouped = new Map<CoreActivity["component"], Resource>();
    const get = (id: CoreActivity["component"]) => {
      let resource = grouped.get(id);
      if (!resource) { resource = { id, receipts: [], tools: [], diagnostics: new Map() }; grouped.set(id, resource); }
      return resource;
    };
    for (const step of steps) {
      step.coreActivities?.forEach(activity => {
        const resource = get(activity.component);
        if (activity.component !== "lsp" || activity.action !== "file_diagnostics" || activity.sources.length !== 1) {
          resource.receipts.push(activity);
          return;
        }
        const path = activity.sources[0];
        const previous = resource.diagnostics.get(path);
        let warning = previous?.warning, resolved = previous?.resolved;
        if (activity.status === "applied") {
          resolved = warning ?? resolved;
          warning = undefined;
        } else if (hasWarning(activity.status)) {
          // Pending checks and cached results cannot erase a known diagnosis.
          warning = activity.status === "pending" ? warning ?? activity : activity;
          resolved = undefined;
        }
        resource.diagnostics.set(path, { current: activity, warning, resolved });
      });
      step.tools.forEach(tool => { const id = componentFor(tool); if (id) get(id).tools.push(tool); });
    }
    return [...grouped.values()];
  }, [steps]);
  if (resources.length === 0) return null;
  const warnings = resources.some(resource => resource.receipts.some(receipt => hasWarning(receipt.status)) || [...resource.diagnostics.values()].some(item => item.warning) || resource.tools.some(tool => tool.status === "error"));

  return <Collapsible className="mt-1 min-w-0">
    <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto min-h-8 cursor-pointer gap-2 px-1 text-[11px] text-muted-foreground hover:bg-transparent hover:text-foreground">
      <Cpu aria-hidden="true" className="size-3.5" />
      <span>Recursos do Core</span>
      <span className="font-mono text-[10px] tabular-nums">· {resources.length}</span>
      {warnings && <TriangleAlert aria-label="Há recursos com avisos" className="size-3.5 text-onedark-yellow" />}
      <ChevronRight aria-hidden="true" className="size-3.5 transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
    </CollapsibleTrigger>
    <CollapsibleContent className="ml-2 border-l border-border/60 py-2 pl-3">
      <ul aria-label="Uso dos recursos do Core" className="flex min-w-0 flex-col gap-4">
        {resources.map(resource => <ResourceDetails key={resource.id} resource={resource} />)}
      </ul>
    </CollapsibleContent>
  </Collapsible>;
}

function ResourceDetails({ resource }: { resource: Resource }) {
  // Keep long runs compact: show the latest state per action, retaining warnings
  // separately so a later success cannot silently erase unavailable checks.
  const latest = [...new Map(resource.receipts.map(receipt => [`${receipt.action}:${receipt.status}`, receipt])).values()];
  const actions = [...new Map(latest.map(receipt => [`${receipt.status}:${receipt.summary}`, receipt])).values()];
  const sources = [...new Set(resource.receipts.flatMap(receipt => receipt.sources))].filter(source => !resource.diagnostics.has(source));
  const successful = resource.tools.filter(tool => tool.status === "completed").length;
  const failed = resource.tools.filter(tool => tool.status === "error").length;
  const pending = resource.tools.length - successful - failed;
  return <li className="min-w-0 text-xs leading-relaxed">
    <div className="mb-1 flex flex-wrap items-center gap-2">
      <span className="font-medium text-foreground">{names[resource.id]}</span>
      {(actions.length > 0 || resource.diagnostics.size > 0) && <Badge variant="outline" className="text-[10px] font-normal text-muted-foreground">Automático</Badge>}
    </div>
    {resource.diagnostics.size > 0 && <DiagnosticDetails diagnostics={resource.diagnostics} />}
    {actions.map(receipt => <p key={`${receipt.action}:${receipt.status}`} className={hasWarning(receipt.status) ? "text-onedark-yellow" : "text-muted-foreground"}>
      <span className="font-medium">{statuses[receipt.status]}: </span>{receipt.summary}
    </p>)}
    {resource.tools.length > 0 && <p className="text-muted-foreground">
      Solicitado pelo agente: {successful} {successful === 1 ? "chamada concluída" : "chamadas concluídas"}
      {pending > 0 ? ` · ${pending} em andamento` : ""}{failed > 0 ? ` · ${failed} não concluída(s)` : ""}.
    </p>}
    {sources.length > 0 && <div className="mt-1 text-muted-foreground">
      <p>Fontes utilizadas:</p>
      <ul className="mt-0.5 space-y-0.5 font-mono text-[10px]">
        {sources.slice(0, 12).map(source => <li key={source} className="break-all">{source}</li>)}
      </ul>
      {sources.length > 12 && <p className="mt-1">Mais {sources.length - 12} fontes registradas no histórico.</p>}
    </div>}
  </li>;
}

function DiagnosticDetails({ diagnostics }: { diagnostics: Map<string, DiagnosticState> }) {
  const entries = [...diagnostics.entries()];
  const checked = entries.filter(([, item]) => ["applied", "reused", "issues"].includes(item.current.status)).length;
  const failed = entries.filter(([, item]) => item.current.status === "unavailable").length;
  const resolved = entries.filter(([, item]) => item.resolved);
  const label = checked === entries.length ? "Verificação concluída" : checked > 0 ? "Verificação parcial" : failed === entries.length ? "Servidor indisponível" : "Verificação pendente";
  return <div className="space-y-2">
    <p className="text-muted-foreground">{label} · {checked}/{entries.length} arquivo(s) verificado(s)</p>
    <ul aria-label="Diagnósticos por arquivo" className="space-y-2">
      {entries.map(([path, item]) => <li key={path} className="min-w-0">
        <p className="break-all font-mono text-[10px] text-muted-foreground">{path}</p>
        <p className={hasWarning(item.current.status) ? "text-onedark-yellow" : "text-muted-foreground"}>
          <span className="font-medium">{item.current.status === "applied" ? "Verificado" : item.current.status === "unavailable" ? "Falha do servidor" : statuses[item.current.status]}: </span>{item.current.summary}
        </p>
        {item.warning && item.warning !== item.current && <p className="text-onedark-yellow">Último aviso ainda não revalidado: {item.warning.summary}</p>}
      </li>)}
    </ul>
    {resolved.length > 0 && <Collapsible>
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto cursor-pointer gap-1 px-0 py-1 text-xs text-muted-foreground">
        {resolved.length} {resolved.length === 1 ? "aviso resolvido" : "avisos resolvidos"} após nova verificação
        <ChevronRight aria-hidden="true" className="size-3 transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className="space-y-1 text-muted-foreground">
          {resolved.map(([path, item]) => <li key={path}><span className="break-all font-mono text-[10px]">{path}</span>: {item.resolved?.summary}</li>)}
        </ul>
      </CollapsibleContent>
    </Collapsible>}
  </div>;
}
