import { Check, ChevronDown, Hammer, Palette, Route, Workflow as WorkflowIcon } from "lucide-react";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import type { Workflow } from "@/core/workflow";

const FLOWS = [
  { value: "standard", title: "Padrão", description: "Da sua instrução à implementação.", icon: Hammer, color: "#61afef" },
  { value: "designer", title: "Designer", description: "Referências, direção visual e interfaces.", icon: Palette, color: "#e06c9f" },
  { value: "planned", title: "Planejado", description: "Planeje antes de construir e validar.", icon: Route, color: "#c678dd" },
  { value: "complete", title: "Completo", description: "Uma equipe da investigação à revisão.", icon: WorkflowIcon, color: "#e5c07b" },
] as const;

export function FlowPicker({ value, onChange, disabled }: { value: Workflow; onChange: (flow: Workflow) => void; disabled?: boolean }) {
  const selected = FLOWS.find(flow => flow.value === value) ?? FLOWS[0];
  const Icon = selected.icon;
  return <DropdownMenu>
    <DropdownMenuTrigger aria-label="Selecionar fluxo" disabled={disabled} className="flex h-7.5 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs font-medium hover:bg-secondary focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50">
      <Icon className="size-3.5 shrink-0" style={{ color: selected.color }} /><span>{selected.title}</span><ChevronDown className="size-3 text-muted-foreground" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" side="top" sideOffset={10} className="instrument-panel w-[380px] max-w-[calc(100vw-32px)] bg-card p-2">
      <DropdownMenuGroup className="grid grid-cols-1 gap-2" aria-label="Fluxos">
        {FLOWS.map(flow => <DropdownMenuItem key={flow.value} aria-label={flow.title} onClick={() => onChange(flow.value)} className="group relative flex cursor-pointer items-center gap-3 rounded-md border p-3 whitespace-normal transition-colors motion-reduce:transition-none" style={{ borderColor: flow.value === value ? `${flow.color}80` : "var(--border)", backgroundColor: flow.value === value ? `${flow.color}12` : undefined }}>
          <span className="flex size-8 items-center justify-center rounded-md border" style={{ color: flow.color, borderColor: `${flow.color}30`, background: `${flow.color}10` }}><flow.icon className="size-4" /></span>
          {flow.value === value && <Check aria-label="Selecionado" className="absolute top-3 right-3 size-3.5" style={{ color: flow.color }} />}
          <span className="min-w-0 flex-1 space-y-1 pr-5"><span className="block text-xs font-medium" style={{ color: flow.color }}>{flow.title}</span><span className="block text-[11px] leading-4 text-muted-foreground">{flow.description}</span></span>
        </DropdownMenuItem>)}
      </DropdownMenuGroup>
    </DropdownMenuContent>
  </DropdownMenu>;
}
