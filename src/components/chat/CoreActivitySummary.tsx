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
const statuses = { applied: "Aplicado", reused: "Reutilizado", unavailable: "Indisponível" };

function componentFor(tool: ToolCallItem): CoreActivity["component"] | undefined {
  if (tool.name.startsWith("ctx_")) return "context-mode";
  if (tool.name.startsWith("design_")) return "open-design";
  if (tool.name.startsWith("context7_")) return "context7";
  if (tool.name.startsWith("beads_")) return "beads";
  if (tool.name.startsWith("lsp_")) return "lsp";
}

type Resource = { id: CoreActivity["component"]; receipts: CoreActivity[]; tools: ToolCallItem[] };

export function CoreActivitySummary({ steps }: { steps: AssistantWorkData["steps"] }) {
  const resources = useMemo(() => {
    const grouped = new Map<CoreActivity["component"], Resource>();
    const get = (id: CoreActivity["component"]) => {
      let resource = grouped.get(id);
      if (!resource) { resource = { id, receipts: [], tools: [] }; grouped.set(id, resource); }
      return resource;
    };
    for (const step of steps) {
      step.coreActivities?.forEach(activity => get(activity.component).receipts.push(activity));
      step.tools.forEach(tool => { const id = componentFor(tool); if (id) get(id).tools.push(tool); });
    }
    return [...grouped.values()];
  }, [steps]);
  if (resources.length === 0) return null;
  const warnings = resources.some(resource => resource.receipts.some(receipt => receipt.status === "unavailable") || resource.tools.some(tool => tool.status === "error"));

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
  const sources = [...new Set(resource.receipts.flatMap(receipt => receipt.sources))];
  const successful = resource.tools.filter(tool => tool.status === "completed").length;
  const failed = resource.tools.filter(tool => tool.status === "error").length;
  const pending = resource.tools.length - successful - failed;
  return <li className="min-w-0 text-xs leading-relaxed">
    <div className="mb-1 flex flex-wrap items-center gap-2">
      <span className="font-medium text-foreground">{names[resource.id]}</span>
      {actions.length > 0 && <Badge variant="outline" className="text-[10px] font-normal text-muted-foreground">Automático</Badge>}
    </div>
    {actions.map(receipt => <p key={`${receipt.action}:${receipt.status}`} className={receipt.status === "unavailable" ? "text-onedark-yellow" : "text-muted-foreground"}>
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
