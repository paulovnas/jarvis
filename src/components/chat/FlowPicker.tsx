import { Check, ChevronDown } from "lucide-react";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import type { CustomAgent, CustomFlow, FlowSelection } from "@/core/workflow-catalog";

import { BUILTIN_FLOWS } from "@/components/agents/workflow-presentation";
import { workflowAppearance } from "@/components/agents/workflow-appearance";
import { agentAppearance, flowAppearance } from "@/core/workflow-appearance";

type PickerOption = (typeof BUILTIN_FLOWS)[number] | {
  value: `custom:${string}` | `agent:${string}`;
  title: string;
  description: string;
  icon: (typeof BUILTIN_FLOWS)[number]["icon"];
  color: string;
};

function PickerItems({ options, value, onChange }: { options: PickerOption[]; value: FlowSelection; onChange: (flow: FlowSelection) => void }) {
  return options.map(option => <DropdownMenuItem key={option.value} aria-label={option.title} onClick={() => onChange(option.value)} className="group relative flex cursor-pointer items-center gap-3 rounded-md border p-3 whitespace-normal transition-colors motion-reduce:transition-none" style={{ borderColor: option.value === value ? `color-mix(in srgb, ${option.color} 50%, transparent)` : "var(--border)", backgroundColor: option.value === value ? `color-mix(in srgb, ${option.color} 7%, transparent)` : undefined }}>
    <span className="flex size-8 items-center justify-center rounded-md border" style={{ color: option.color, borderColor: `color-mix(in srgb, ${option.color} 20%, transparent)`, background: `color-mix(in srgb, ${option.color} 6%, transparent)` }}><option.icon className="size-4" /></span>
    {option.value === value && <Check aria-label="Selecionado" className="absolute top-3 right-3 size-3.5" style={{ color: option.color }} />}
    <span className="min-w-0 flex-1 space-y-1 pr-5"><span className="block text-xs font-medium" style={{ color: option.color }}>{option.title}</span><span className="block text-[11px] leading-4 text-muted-foreground">{option.description}</span></span>
  </DropdownMenuItem>);
}

export function FlowPicker({ value, onChange, disabled, customFlows = [], customAgents = [] }: { value: FlowSelection; onChange: (flow: FlowSelection) => void; disabled?: boolean; customFlows?: CustomFlow[]; customAgents?: CustomAgent[] }) {
  const savedFlows: PickerOption[] = customFlows.map(flow => { const { Icon, color } = workflowAppearance(flow.appearance, flowAppearance); return { value: `custom:${flow.id}`, title: flow.name, description: flow.description || `${flow.steps.length} etapas personalizadas`, icon: Icon, color }; });
  const individualAgents: PickerOption[] = customAgents.filter(agent => agent.usage === "solo" || agent.usage === "mixed").map(agent => { const { Icon, color } = workflowAppearance(agent.appearance, agentAppearance); return { value: `agent:${agent.id}`, title: agent.name, description: agent.description || "Agente personalizado para uso direto", icon: Icon, color }; });
  const options: PickerOption[] = [...BUILTIN_FLOWS, ...savedFlows, ...individualAgents];
  const selected = options.find(option => option.value === value) ?? { ...BUILTIN_FLOWS[0], title: "Opção indisponível" };
  const Icon = selected.icon;
  return <DropdownMenu>
    <DropdownMenuTrigger aria-label="Selecionar fluxo" disabled={disabled} className="flex h-7.5 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs font-medium hover:bg-secondary focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50">
      <Icon className="size-3.5 shrink-0" style={{ color: selected.color }} /><span>{selected.title}</span><ChevronDown className="size-3 text-muted-foreground" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" side="top" sideOffset={10} className="instrument-panel max-h-[70dvh] overflow-y-auto w-[380px] max-w-[calc(100vw-32px)] bg-card p-2">
      <DropdownMenuGroup className="grid grid-cols-1 gap-2" aria-label="Fluxos Jarvis">
        <DropdownMenuLabel className="micro-label px-2 py-1.5 text-muted-foreground">Fluxos Jarvis</DropdownMenuLabel>
        <PickerItems options={[...BUILTIN_FLOWS]} value={value} onChange={onChange} />
      </DropdownMenuGroup>
      {savedFlows.length > 0 && <><DropdownMenuSeparator className="my-2" /><DropdownMenuGroup className="grid grid-cols-1 gap-2" aria-label="Fluxos personalizados"><DropdownMenuLabel className="micro-label px-2 py-1.5 text-muted-foreground">Fluxos personalizados</DropdownMenuLabel><PickerItems options={savedFlows} value={value} onChange={onChange} /></DropdownMenuGroup></>}
      {individualAgents.length > 0 && <><DropdownMenuSeparator className="my-2" /><DropdownMenuGroup className="grid grid-cols-1 gap-2" aria-label="Agentes individuais"><DropdownMenuLabel className="micro-label px-2 py-1.5 text-muted-foreground">Agentes individuais</DropdownMenuLabel><PickerItems options={individualAgents} value={value} onChange={onChange} /></DropdownMenuGroup></>}
    </DropdownMenuContent>
  </DropdownMenu>;
}
