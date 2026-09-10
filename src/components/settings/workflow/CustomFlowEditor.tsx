import { lazy, Suspense, useId, useState } from "react";
import { CircleAlert, CircleCheck, Flag, Plus, Trash2 } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { customId, isBuiltinAgent, validateGraph, type CustomFlow, type FlowAgent, type WorkflowStep } from "@/core/workflow-catalog";
import { ChoiceField, DiscardDialog } from "./WorkflowFields";
import { flowAppearance } from "@/core/workflow-appearance";
import { AppearancePicker } from "./AppearancePicker";

const Canvas = lazy(() => import("./WorkflowCanvas"));
export function CustomFlowEditor({ initial, agents, saving, creating, onSave, onClose }: { initial: CustomFlow; agents: FlowAgent[]; saving: boolean; creating: boolean; onSave: (flow: CustomFlow) => Promise<boolean>; onClose: () => void }) {
  const [flow, setFlow] = useState(initial);
  const [selected, setSelected] = useState<string | null>(initial.entry || null);
  const [addAgent, setAddAgent] = useState(agents[0]?.id ?? "");
  const [discard, setDiscard] = useState(false);
  const validationMessageId = useId();
  const dirty = JSON.stringify(initial) !== JSON.stringify(flow);
  const close = () => { if (saving) return; if (dirty) setDiscard(true); else onClose(); };
  const patch = (next: Partial<CustomFlow>) => setFlow(current => ({ ...current, ...next }));
  const selectedStep = flow.steps.find(step => step.id === selected);
  const patchStep = (next: Partial<WorkflowStep>) => patch({ steps: flow.steps.map(step => step.id === selected ? { ...step, ...next } : step) });
  const validation = validateGraph(flow, agents);
  const agentLabel = (agent: FlowAgent) => `${agent.name}${isBuiltinAgent(agent) ? " · Jarvis" : ""}`;
  const agentOptions = agents.map(agent => ({ value: agent.id, label: agentLabel(agent) }));
  const stepOptions = flow.steps.map((step, i) => ({ value: step.id, label: `${i + 1}. ${agents.find(a => a.id === step.agentId)?.name ?? "Agente indisponível"}` }));
  const add = () => {
    if (!addAgent || flow.steps.length >= 24 || saving) return;
    const step: WorkflowStep = { id: customId(), agentId: addAgent, instructions: "", position: { x: 50 + flow.steps.length % 3 * 300, y: 50 + Math.floor(flow.steps.length / 3) * 230 }, next: null, onRework: null };
    patch({ steps: [...flow.steps, step], entry: flow.entry || step.id }); setSelected(step.id);
  };
  const remove = () => {
    patch({ entry: flow.entry === selected ? "" : flow.entry, steps: flow.steps.filter(step => step.id !== selected).map(step => ({ ...step, next: step.next === selected ? null : step.next, onRework: step.onRework === selected ? null : step.onRework })) });
    setSelected(null);
  };
  return <><Dialog open onOpenChange={open => { if (!open) close(); }}><DialogContent className="dark flex h-[90dvh] w-[96vw] flex-col gap-0 overflow-hidden p-0 sm:max-w-[1240px]">
    <DialogHeader className="shrink-0 border-b border-border bg-card p-5 pr-12"><DialogTitle>{creating ? "Adicionar fluxo" : "Editar fluxo"}</DialogTitle><DialogDescription>Arraste os blocos e conecte as saídas. Um agente executa por vez, começando pelo bloco marcado como início.</DialogDescription></DialogHeader>
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 md:flex-row md:overflow-hidden">
      <aside className="w-full shrink-0 space-y-4 md:w-56 md:overflow-y-auto md:pr-2">
        <div className="space-y-1.5"><Label htmlFor="custom-flow-name">Nome</Label><Input id="custom-flow-name" value={flow.name} maxLength={100} disabled={saving} onChange={e => patch({ name: e.target.value })} /></div>
        <div className="space-y-1.5"><Label htmlFor="custom-flow-description">Descrição</Label><Textarea id="custom-flow-description" value={flow.description} maxLength={500} disabled={saving} onChange={e => patch({ description: e.target.value })} /></div>
        <AppearancePicker value={flow.appearance ?? flowAppearance} onChange={appearance => patch({ appearance })} disabled={saving} />
        <div className="space-y-2 rounded-md border border-border bg-sidebar p-3"><ChoiceField label="Adicionar agente ao canvas" value={addAgent} disabled={saving || !agents.length} options={agentOptions} onChange={setAddAgent} /><Button variant="outline" size="sm" className="w-full cursor-pointer text-xs" disabled={saving || !addAgent || flow.steps.length >= 24} onClick={add}><Plus />Adicionar bloco</Button>{!agents.length && <p className="text-xs text-muted-foreground">Crie um agente na aba Agentes para começar.</p>}</div>
        <ChoiceField label="Bloco inicial" value={flow.entry || "none"} disabled={saving} options={[{ value: "none", label: "Selecione o início" }, ...stepOptions]} onChange={value => patch({ entry: value === "none" ? "" : value })} />
        <div className="space-y-1.5"><Label htmlFor="custom-flow-limit">Limite de execuções</Label><Input id="custom-flow-limit" type="number" min={Math.max(1, flow.steps.length)} max={48} value={flow.maxSteps} disabled={saving} onChange={e => patch({ maxSteps: Number(e.target.value) })} /><p className="text-[11px] leading-4 text-muted-foreground">Inclui retornos para correção. Ao atingir o limite, o fluxo para e informa o motivo.</p></div>
        <p className="text-[11px] leading-5 text-muted-foreground">A saída azul segue após conclusão ou aprovação. A vermelha retorna quando o agente solicita correções. Sem saída azul, o fluxo termina.</p>
      </aside>
      <div className="flex min-h-[440px] min-w-0 flex-1 flex-col gap-3 md:overflow-y-auto">
        <div className="min-h-72 flex-1"><Suspense fallback={<Skeleton className="h-full min-h-72" aria-label="Carregando canvas" />}><Canvas flow={flow} agents={agents} selected={selected} onSelect={setSelected} onChange={setFlow} disabled={saving} /></Suspense></div>
        <div className="space-y-3 rounded-md border border-border bg-card p-3">
          <ChoiceField label="Editar bloco" value={selected ?? "none"} options={[{ value: "none", label: "Selecione um bloco no canvas" }, ...stepOptions]} onChange={value => setSelected(value === "none" ? null : value)} />
          {selectedStep && <><div className="grid gap-3 sm:grid-cols-3"><ChoiceField label="Agente vinculado" value={selectedStep.agentId} disabled={saving} options={agentOptions} onChange={agentId => patchStep({ agentId })} /><ChoiceField label="Ao concluir" value={selectedStep.next ?? "end"} disabled={saving} options={[{ value: "end", label: "Finalizar fluxo" }, ...stepOptions.filter(s => s.value !== selected)]} onChange={value => patchStep({ next: value === "end" ? null : value })} /><ChoiceField label="Ao solicitar correção" value={selectedStep.onRework ?? "stop"} disabled={saving} options={[{ value: "stop", label: "Parar e informar" }, ...stepOptions]} onChange={value => patchStep({ onRework: value === "stop" ? null : value })} /></div>
            <div className="space-y-1.5"><Label htmlFor="custom-step-instructions">Instruções desta etapa (opcional)</Label><Textarea id="custom-step-instructions" className="min-h-16 font-mono text-xs" value={selectedStep.instructions} maxLength={8000} disabled={saving} onChange={e => patchStep({ instructions: e.target.value })} /></div>
            <div className="flex justify-between gap-2"><Button variant="outline" size="sm" className="cursor-pointer text-xs" disabled={saving || selected === flow.entry} onClick={() => patch({ entry: selectedStep.id })}><Flag />Marcar como início</Button><Button variant="ghost" size="sm" className="cursor-pointer text-xs text-destructive" disabled={saving} onClick={remove}><Trash2 />Remover bloco</Button></div>
          </>}
        </div>
      </div>
    </div>
    <DialogFooter className="m-0 shrink-0 flex-col gap-4 border-t border-border bg-card p-4 sm:flex-row sm:items-center sm:justify-between sm:px-5">
      <Alert
        id={validationMessageId}
        role="status"
        aria-live="polite"
        aria-atomic="true"
        className={`min-w-0 px-3 py-2.5 sm:flex-1 ${validation ? "border-onedark-yellow/25 bg-onedark-yellow/5 text-onedark-yellow" : "border-onedark-green/25 bg-onedark-green/5 text-onedark-green"}`}
      >
        {validation ? <CircleAlert aria-hidden="true" /> : <CircleCheck aria-hidden="true" />}
        <AlertTitle className="text-sm">{validation ? "Antes de salvar" : "Pronto para salvar"}</AlertTitle>
        <AlertDescription className="text-xs leading-relaxed text-foreground">
          {validation ?? `${flow.steps.length} ${flow.steps.length === 1 ? "etapa conectada" : "etapas conectadas"}.`}
        </AlertDescription>
      </Alert>
      <div className="flex w-full shrink-0 items-center justify-end gap-3 sm:w-auto">
        <Button variant="outline" className="h-9 flex-1 cursor-pointer sm:flex-none" disabled={saving} onClick={close}>Cancelar</Button>
        <Button
          className="h-9 flex-1 cursor-pointer sm:flex-none"
          aria-describedby={validationMessageId}
          disabled={saving || Boolean(validation)}
          onClick={() => void onSave({ ...flow, name: flow.name.trim() }).then(saved => { if (saved) onClose(); })}
        >{saving ? "Salvando…" : "Salvar fluxo"}</Button>
      </div>
    </DialogFooter>
  </DialogContent></Dialog><DiscardDialog open={discard} onOpenChange={setDiscard} onDiscard={onClose} /></>;
}
