import { Check, ChevronDown } from "lucide-react";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import type { CustomFlow, FlowSelection } from "@/core/workflow-catalog";

import { BUILTIN_FLOWS } from "@/components/agents/workflow-presentation";
import { workflowAppearance } from "@/components/agents/workflow-appearance";
import { flowAppearance } from "@/core/workflow-appearance";

export function FlowPicker({ value, onChange, disabled, customFlows = [] }: { value: FlowSelection; onChange: (flow: FlowSelection) => void; disabled?: boolean; customFlows?: CustomFlow[] }) {
  const flows = [...BUILTIN_FLOWS, ...customFlows.map(flow => { const { Icon, color } = workflowAppearance(flow.appearance, flowAppearance); return { value: `custom:${flow.id}` as const, title: flow.name, description: flow.description || `${flow.steps.length} etapas personalizadas`, icon: Icon, color }; })];
  const selected = flows.find(flow => flow.value === value) ?? { ...BUILTIN_FLOWS[0], title: "Fluxo indisponível" };
  const Icon = selected.icon;
  return <DropdownMenu>
    <DropdownMenuTrigger aria-label="Selecionar fluxo" disabled={disabled} className="flex h-7.5 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs font-medium hover:bg-secondary focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50">
      <Icon className="size-3.5 shrink-0" style={{ color: selected.color }} /><span>{selected.title}</span><ChevronDown className="size-3 text-muted-foreground" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" side="top" sideOffset={10} className="instrument-panel max-h-[70dvh] overflow-y-auto w-[380px] max-w-[calc(100vw-32px)] bg-card p-2">
      <DropdownMenuGroup className="grid grid-cols-1 gap-2" aria-label="Fluxos">
        {flows.map(flow => <DropdownMenuItem key={flow.value} aria-label={flow.title} onClick={() => onChange(flow.value)} className="group relative flex cursor-pointer items-center gap-3 rounded-md border p-3 whitespace-normal transition-colors motion-reduce:transition-none" style={{ borderColor: flow.value === value ? `color-mix(in srgb, ${flow.color} 50%, transparent)` : "var(--border)", backgroundColor: flow.value === value ? `color-mix(in srgb, ${flow.color} 7%, transparent)` : undefined }}>
          <span className="flex size-8 items-center justify-center rounded-md border" style={{ color: flow.color, borderColor: `color-mix(in srgb, ${flow.color} 20%, transparent)`, background: `color-mix(in srgb, ${flow.color} 6%, transparent)` }}><flow.icon className="size-4" /></span>
          {flow.value === value && <Check aria-label="Selecionado" className="absolute top-3 right-3 size-3.5" style={{ color: flow.color }} />}
          <span className="min-w-0 flex-1 space-y-1 pr-5"><span className="block text-xs font-medium" style={{ color: flow.color }}>{flow.title}</span><span className="block text-[11px] leading-4 text-muted-foreground">{flow.description}</span></span>
        </DropdownMenuItem>)}
      </DropdownMenuGroup>
    </DropdownMenuContent>
  </DropdownMenu>;
}
