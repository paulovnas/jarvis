import { useEffect, useRef, useState } from "react";
import { ChevronDown, X } from "lucide-react";
import { CommandInput } from "@/components/TextInput";
import { Button } from "@/components/ui/button";
import { Command, CommandEmpty, CommandGroup, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Hint } from "@/components/ui/hint";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { BuiltinAgentDefinition, CustomAgent, CustomFlow, FlowSelection } from "@/core/workflow-catalog";

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

const filters = [{ value: "all", label: "Todos" }, { value: "flows", label: "Fluxos" }, { value: "agents", label: "Agentes" }] as const;
type PickerFilter = (typeof filters)[number]["value"];

// cmdk does not initialize the active descendant for a controlled preselection.
function syncActiveDescendant(input: HTMLInputElement | null, list: HTMLDivElement | null) {
  const active = list?.querySelector('[aria-selected="true"]');
  for (const element of [input, list]) {
    if (active) element?.setAttribute("aria-activedescendant", active.id);
    else element?.removeAttribute("aria-activedescendant");
  }
}

export function FlowPicker({ value, onChange, disabled, customFlows = [], customAgents = [], builtinAgents = [] }: { value: FlowSelection; onChange: (flow: FlowSelection) => void; disabled?: boolean; customFlows?: CustomFlow[]; customAgents?: CustomAgent[]; builtinAgents?: BuiltinAgentDefinition[] }) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<PickerFilter>("all");
  const [highlighted, setHighlighted] = useState<string>(value);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (open) syncActiveDescendant(searchRef.current, listRef.current);
  }, [highlighted, open, search, filter]);
  const savedFlows: PickerOption[] = customFlows.map(flow => { const { Icon, color } = workflowAppearance(flow.appearance, flowAppearance); return { value: `custom:${flow.id}`, title: flow.name, description: flow.description || `${flow.steps.length} etapas personalizadas`, icon: Icon, color }; });
  const nativeAgents: PickerOption[] = builtinAgents.filter(agent => agent.usage === "mixed").map(agent => { const { Icon, color } = workflowAppearance(agent.appearance, agentAppearance); return { value: `agent:${agent.id}`, title: agent.name, description: agent.description, icon: Icon, color }; });
  const customIndividualAgents: PickerOption[] = customAgents.filter(agent => agent.usage === "solo" || agent.usage === "mixed").map(agent => { const { Icon, color } = workflowAppearance(agent.appearance, agentAppearance); return { value: `agent:${agent.id}`, title: agent.name, description: agent.description || "Agente personalizado para uso direto", icon: Icon, color }; });
  const groups = [
    { title: "Fluxos Jarvis", kind: "flows", options: [...BUILTIN_FLOWS] },
    { title: "Fluxos personalizados", kind: "flows", options: savedFlows },
    { title: "Agentes Jarvis", kind: "agents", options: nativeAgents },
    { title: "Agentes personalizados", kind: "agents", options: customIndividualAgents },
  ];
  const options = groups.flatMap(group => group.options);
  const current = options.find(option => option.value === value);
  const selected = current ?? { ...BUILTIN_FLOWS[0], title: "Opção indisponível" };
  const detailGroup = groups.find(group => group.options.some(option => option.value === highlighted));
  const detail = detailGroup?.options.find(option => option.value === highlighted);
  const Icon = selected.icon;
  const changeOpen = (next: boolean) => {
    setOpen(next);
    if (next) { setSearch(""); setFilter("all"); setHighlighted(current?.value ?? BUILTIN_FLOWS[0].value); }
  };

  return <Popover open={open} onOpenChange={changeOpen} onOpenChangeComplete={next => {
    if (next) {
      syncActiveDescendant(searchRef.current, listRef.current);
      listRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
    }
  }}>
    <PopoverTrigger ref={triggerRef} aria-label="Selecionar fluxo" disabled={disabled} onKeyDown={event => { if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); changeOpen(true); } }} className="flex h-7.5 min-w-0 max-w-48 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs font-medium hover:bg-secondary focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50">
      <Icon className="size-3.5 shrink-0" style={{ color: selected.color }} /><Hint content={selected.title} whenTruncated><span className="min-w-0 truncate">{selected.title}</span></Hint><ChevronDown className="size-3 shrink-0 text-muted-foreground" />
    </PopoverTrigger>
    <PopoverContent align="start" side="top" sideOffset={10} initialFocus={searchRef} finalFocus={triggerRef} aria-label="Fluxos e agentes" className="instrument-panel max-h-[min(520px,70dvh,var(--available-height))] w-[400px] max-w-[calc(100vw-32px)] gap-0 overflow-hidden bg-card p-0">
      <Command label="Buscar fluxo ou agente" value={highlighted} onValueChange={setHighlighted} className="min-h-0 flex-1 rounded-none! bg-transparent p-1">
        <CommandInput ref={searchRef} value={search} onValueChange={setSearch} placeholder="Buscar fluxo ou agente" aria-label="Buscar fluxo ou agente" className="text-xs" />
        <ToggleGroup aria-label="Filtrar opções" value={[filter]} onValueChange={values => {
          const next = filters.find(item => item.value === values[0]);
          if (next) setFilter(next.value);
        }} onKeyDown={event => { if (event.key !== "Escape") event.stopPropagation(); }} size="sm" className="shrink-0 gap-1 px-2 py-2">
          {filters.map(item => <ToggleGroupItem key={item.value} value={item.value} className="h-6 cursor-pointer rounded-md px-2.5 text-[11px] data-pressed:bg-secondary data-pressed:text-foreground">{item.label}</ToggleGroupItem>)}
        </ToggleGroup>
        <CommandList ref={listRef} label="Opções de fluxo e agente" className="min-h-0 max-h-80 flex-1">
          <CommandEmpty className="px-3 py-8 text-xs text-muted-foreground">Nenhum fluxo ou agente encontrado.</CommandEmpty>
          {groups.filter(group => group.options.length > 0 && (filter === "all" || filter === group.kind)).map(group => <CommandGroup key={group.title} heading={group.title} className="**:[[cmdk-group-heading]]:text-[10px] **:[[cmdk-group-heading]]:font-semibold **:[[cmdk-group-heading]]:tracking-widest **:[[cmdk-group-heading]]:uppercase">
            {group.options.map(option => <CommandItem key={option.value} value={option.value} keywords={[option.title, option.description]} aria-label={option.title} aria-current={option.value === value ? "true" : undefined} data-checked={option.value === value} onSelect={() => { onChange(option.value); setOpen(false); }} className="h-12 cursor-pointer gap-2.5 rounded-md border border-transparent px-2 data-[selected=true]:bg-secondary [&>svg:last-child]:size-3.5" style={{ color: option.color, borderColor: option.value === value ? `color-mix(in srgb, ${option.color} 35%, transparent)` : undefined }}>
              <span className="flex size-6 shrink-0 items-center justify-center rounded-md bg-secondary/60"><option.icon className="size-3.5" /></span>
              <span className="min-w-0 flex-1"><span className="block truncate text-xs font-medium">{option.title}</span><span className="block truncate text-[11px] leading-4 text-muted-foreground">{option.description}</span></span>
            </CommandItem>)}
          </CommandGroup>)}
        </CommandList>
      </Command>
      <div aria-label="Detalhes da opção em destaque" className="shrink-0 space-y-1.5 border-t border-border bg-secondary/25 px-3 py-2.5">
        <div className="flex items-center justify-between gap-2">
          <div className="min-w-0"><p className="truncate text-xs font-medium" style={{ color: detail?.color }}>{detail?.title ?? "Sem resultados"}</p><span className="micro-label text-muted-foreground">{detailGroup?.title}</span></div>
          <Dialog>
            <DialogTrigger disabled={!detail} render={<Button variant="ghost" size="sm" className="h-6 shrink-0 cursor-pointer px-1.5 text-[11px] text-primary" />}>Ver detalhes</DialogTrigger>
            <DialogContent showCloseButton={false} className="instrument-panel max-h-[70dvh] overflow-y-auto sm:max-w-lg">
              <DialogHeader className="min-w-0 pr-6"><DialogTitle className="break-words leading-5" style={{ color: detail?.color }}>{detail?.title}</DialogTitle><DialogDescription>{detailGroup?.title}</DialogDescription></DialogHeader>
              <p className="whitespace-pre-wrap break-words text-sm leading-6">{detail?.description}</p>
              <DialogClose render={<Button variant="ghost" size="icon-sm" aria-label="Fechar detalhes" className="absolute top-2 right-2 cursor-pointer" />}><X /></DialogClose>
            </DialogContent>
          </Dialog>
        </div>
        <p className="line-clamp-2 h-8 text-[11px] leading-4 text-muted-foreground">{detail?.description ?? "Tente outro nome ou descrição."}</p>
      </div>
    </PopoverContent>
  </Popover>;
}
