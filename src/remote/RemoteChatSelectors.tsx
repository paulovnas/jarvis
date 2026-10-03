import { useRef, useState } from "react";
import { ChevronDown, Workflow, X, type LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Command, CommandEmpty, CommandGroup, CommandItem, CommandList } from "@/components/ui/command";
import { CommandInput } from "@/components/TextInput";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { BUILTIN_FLOWS } from "@/components/agents/workflow-presentation";
import { workflowAppearance } from "@/components/agents/workflow-appearance";
import type { ProviderModelGroup } from "@/components/chat/ModelPicker";
import { agentAppearance, flowAppearance } from "@/core/workflow-appearance";
import { executionChoice, executionSelection, executorOf, sameExecutionTarget, selectModelChoice, type ExecutionChoice } from "@/core/executors";
import { defaultReasoning, reasoningLabel, selectableReasoningLevels } from "@/core/reasoning";
import type { ModelChoice } from "@/core/provider-references";
import type { FlowSelection, WorkflowCatalog } from "@/core/workflow-catalog";
import { aliasSuffix } from "@/core/provider-usage";
import { cn } from "@/lib/utils";
import { RemoteProviderIcon } from "./RemoteProviderIcon";

export interface RemoteChatSelectorsProps {
  catalog: Pick<WorkflowCatalog, "flows" | "agents" | "builtinAgents">;
  modelGroups: ProviderModelGroup[];
  flow: FlowSelection;
  choice: ModelChoice | null;
  modelProblem?: string | null;
  disabled?: boolean;
  flowDisabled?: boolean;
  onFlowChange: (flow: FlowSelection) => boolean | void | Promise<boolean | void>;
  onModelChange: (choice: ModelChoice) => boolean | void | Promise<boolean | void>;
}

interface FlowOption {
  value: FlowSelection;
  title: string;
  description: string;
  icon: LucideIcon;
}

function findModel(groups: ProviderModelGroup[], choice?: ExecutionChoice | null) {
  const selection = executionSelection(choice);
  const group = groups.find(group => executorOf(group) === executorOf(choice) && group.models.some(model => model.value === selection?.model));
  return { group, model: group?.models.find(model => model.value === selection?.model) };
}

export function RemoteChatSelectors({ catalog, modelGroups, flow, choice, modelProblem, disabled = false, flowDisabled = false, onFlowChange, onModelChange }: RemoteChatSelectorsProps) {
  const [panel, setPanel] = useState<"flow" | "model" | null>(null);
  const [draft, setDraft] = useState<ModelChoice | null>(choice);
  const [slot, setSlot] = useState<"primary" | "secondary">("primary");
  const [saving, setSaving] = useState(false);
  const mutationLock = useRef(false);
  const unavailable = disabled || saving;
  const groups: { label: string; options: FlowOption[] }[] = [
    { label: "Fluxos Jarvis", options: [...BUILTIN_FLOWS] },
    { label: "Fluxos personalizados", options: catalog.flows.map<FlowOption>(item => ({ value: `custom:${item.id}`, title: item.name, description: item.description, icon: workflowAppearance(item.appearance, flowAppearance).Icon })) },
    { label: "Agentes Jarvis", options: catalog.builtinAgents.filter(item => item.usage === "mixed").map<FlowOption>(item => ({ value: `agent:${item.id}`, title: item.name, description: item.description, icon: workflowAppearance(item.appearance, agentAppearance).Icon })) },
    { label: "Agentes personalizados", options: catalog.agents.filter(item => item.usage !== "flow_only").map<FlowOption>(item => ({ value: `agent:${item.id}`, title: item.name, description: item.description, icon: workflowAppearance(item.appearance, agentAppearance).Icon })) },
  ];
  const currentFlow = groups.flatMap(group => group.options).find(item => item.value === flow);
  const FlowIcon = currentFlow?.icon ?? Workflow;
  const current = findModel(modelGroups, choice);
  const activeChoice = slot === "primary" ? draft : draft?.fallback;
  const active = findModel(modelGroups, activeChoice);
  const levels = selectableReasoningLevels(active.model?.reasoningLevels ?? []);
  const validTarget = (target: ExecutionChoice) => {
    const { model } = findModel(modelGroups, target);
    return Boolean(model && (!target.reasoning || selectableReasoningLevels(model.reasoningLevels).includes(target.reasoning)));
  };
  const validDraft = draft && validTarget(draft) && (!draft.fallback || (validTarget(draft.fallback) && !sameExecutionTarget(draft, draft.fallback)));
  const apply = async (action: () => ReturnType<RemoteChatSelectorsProps["onModelChange"]>) => {
    if (unavailable || mutationLock.current) return;
    mutationLock.current = true; setSaving(true);
    try { if (await action() !== false) setPanel(null); }
    finally { mutationLock.current = false; setSaving(false); }
  };
  const setReasoning = (reasoning: string) => {
    if (!draft || !activeChoice || unavailable) return;
    setDraft(slot === "primary" ? { ...draft, reasoning } : { ...draft, fallback: { ...activeChoice, reasoning } });
  };

  return <>
    <div className="remote-chat-selectors flex min-w-0 items-center gap-1">
      <Button type="button" variant="ghost" size="sm" disabled={unavailable || flowDisabled} aria-label="Selecionar fluxo ou agente" className="h-11 min-w-0 flex-1 cursor-pointer justify-start" onClick={() => setPanel("flow")}>
        <FlowIcon data-icon="inline-start" /><span className="truncate">{currentFlow?.title ?? "Opção indisponível"}</span><ChevronDown data-icon="inline-end" />
      </Button>
      <Button type="button" variant="ghost" size="sm" disabled={unavailable} aria-label="Selecionar modelo de IA" aria-invalid={Boolean(modelProblem) || !current.model || undefined} className={cn("h-11 min-w-0 flex-1 cursor-pointer justify-start font-mono", (modelProblem || !current.model) && "border border-destructive text-destructive")} onClick={() => { setDraft(choice); setSlot("primary"); setPanel("model"); }}>
        {current.group && <RemoteProviderIcon kind={current.group.executor === "claude" ? "claude-code" : current.group.providerKind ?? "custom"} />}
        <span className="truncate">{current.model?.label ?? "Selecionar modelo"}</span><ChevronDown data-icon="inline-end" />
      </Button>
    </div>
    <Sheet open={panel !== null} onOpenChange={open => { if (!open && !saving) setPanel(null); }}>
      <SheetContent side="bottom" showCloseButton={false} className="remote-selector-sheet h-[min(640px,85dvh,var(--remote-height,100dvh))]! gap-0 overflow-hidden rounded-t-xl">
        <SheetHeader className="shrink-0 pr-16">
          <SheetTitle>{panel === "flow" ? "Fluxos e agentes" : "Modelo desta conversa"}</SheetTitle>
          <SheetDescription>{panel === "flow" ? "Escolha quem recebe sua próxima mensagem." : "A seleção fica salva apenas neste chat."}</SheetDescription>
        </SheetHeader>
        <SheetClose disabled={saving} render={<Button type="button" variant="ghost" size="icon" aria-label="Fechar seleção" className="absolute top-3 right-3 cursor-pointer" />}><X /></SheetClose>
        {panel === "flow" ? <Command label="Buscar fluxo ou agente" className="min-h-0 flex-1 rounded-none!">
          <CommandInput aria-label="Buscar fluxo ou agente" placeholder="Buscar fluxo ou agente" disabled={unavailable || flowDisabled} />
          <CommandList className="min-h-0 max-h-none flex-1">
            <CommandEmpty>Nenhum fluxo ou agente encontrado.</CommandEmpty>
            {groups.filter(group => group.options.length).map(group => <CommandGroup key={group.label} heading={group.label}>
              {group.options.map(item => <CommandItem key={item.value} aria-label={item.title} value={`${item.value} ${item.title}`} keywords={[item.description]} disabled={unavailable || flowDisabled} data-checked={item.value === flow} className="min-h-11 cursor-pointer py-2" onSelect={() => { if (!flowDisabled) void apply(() => onFlowChange(item.value)); }}>
                <item.icon /><span className="min-w-0 flex-1"><span className="block truncate">{item.title}</span><span className="block truncate text-xs text-muted-foreground">{item.description}</span></span>
              </CommandItem>)}
            </CommandGroup>)}
          </CommandList>
        </Command> : <>
          <div className="remote-model-options min-h-0 flex-1 overflow-y-auto pb-4">
          <FieldGroup className="shrink-0 gap-2 px-4 pb-2">
            <Field>
              <ToggleGroup aria-label="Modelo principal ou secundário" value={[slot]} onValueChange={values => { if (!unavailable && (values[0] === "primary" || values[0] === "secondary")) setSlot(values[0]); }} disabled={unavailable} className="w-full">
                <ToggleGroupItem value="primary" className="min-h-11 flex-1 cursor-pointer">Principal</ToggleGroupItem>
                <ToggleGroupItem value="secondary" disabled={!draft} className="min-h-11 flex-1 cursor-pointer">Secundário</ToggleGroupItem>
              </ToggleGroup>
              <FieldLabel className="min-w-0 truncate font-mono text-xs">{active.model ? `${aliasSuffix(active.group?.provider ?? "")} · ${active.model.label}` : slot === "secondary" ? "Nenhum modelo secundário" : "Escolha um modelo"}</FieldLabel>
            </Field>
          </FieldGroup>
          <Command key={slot} label="Buscar modelo" className="h-auto! w-full rounded-none!">
            <CommandInput aria-label="Buscar modelo" placeholder="Buscar provedor ou modelo" disabled={unavailable} />
            <CommandList className="max-h-none overflow-y-visible">
              {modelProblem && <Alert variant="destructive" className="my-2"><AlertDescription>{modelProblem}</AlertDescription></Alert>}
              <CommandEmpty>Nenhum modelo encontrado.</CommandEmpty>
              {slot === "secondary" && <CommandGroup><CommandItem value="none" disabled={unavailable} data-checked={!draft?.fallback} className="min-h-11 cursor-pointer" onSelect={() => setDraft(current => current ? { ...current, fallback: null } : current)}>Nenhum</CommandItem></CommandGroup>}
              {modelGroups.map(group => <CommandGroup key={`${group.executor ?? "jarvis"}:${group.provider}`} heading={group.provider}>
                {group.models.map(model => <CommandItem key={model.value} aria-label={`${group.provider} · ${model.label}`} value={`${group.executor ?? "jarvis"} ${model.value}`} keywords={[group.provider, model.label]} disabled={unavailable} data-checked={active.group === group && active.model?.value === model.value} className="min-h-11 cursor-pointer" onSelect={() => setDraft(current => selectModelChoice(current, executionChoice({ executor: group.executor, model: model.value, reasoning: defaultReasoning(model) }), slot))}>
                  <RemoteProviderIcon kind={group.executor === "claude" ? "claude-code" : group.providerKind ?? "custom"} /><span className="min-w-0 flex-1 truncate">{model.label}</span>
                </CommandItem>)}
              </CommandGroup>)}
            </CommandList>
          </Command>
            {levels.length > 0 && <FieldGroup className="px-4 pt-4"><Field><FieldLabel>Raciocínio</FieldLabel><ToggleGroup aria-label="Nível de raciocínio" value={activeChoice?.reasoning ? [activeChoice.reasoning] : []} disabled={unavailable} onValueChange={values => { if (values[0]) setReasoning(values[0]); }} className="flex flex-wrap justify-start">
              {levels.map(level => <ToggleGroupItem key={level} value={level} className="min-h-11 cursor-pointer px-3">{reasoningLabel(level)}</ToggleGroupItem>)}
            </ToggleGroup></Field></FieldGroup>}
            {!modelGroups.some(group => group.models.length) && <div className="px-4"><Alert><AlertDescription>Nenhum modelo conectado. Revise os provedores no Jarvis.</AlertDescription></Alert></div>}
          </div>
          <SheetFooter className="shrink-0 border-t border-border">
            <Button type="button" disabled={unavailable || !validDraft} className="min-h-11 cursor-pointer" onClick={() => { if (draft) void apply(() => onModelChange(draft)); }}>{saving ? "Salvando…" : "Aplicar modelo"}</Button>
          </SheetFooter>
        </>}
      </SheetContent>
    </Sheet>
  </>;
}
