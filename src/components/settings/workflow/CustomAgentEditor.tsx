import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { ModelPicker } from "@/components/chat/ModelPicker";
import { CAPABILITY_LABELS, type CustomAgent } from "@/core/workflow-catalog";
import type { ProviderAccount } from "@/core/provider-accounts";
import { accountGroups } from "./workflow-models";
import { ChoiceField, DiscardDialog } from "./WorkflowFields";
import { modelProblem } from "@/core/provider-references";
import { useModelProblemNotice } from "@/hooks/use-provider-references";
import { agentAppearance } from "@/core/workflow-appearance";
import { AppearancePicker } from "./AppearancePicker";

export function CustomAgentEditor({ initial, accounts, saving, onSave, onClose, creating }: { initial: CustomAgent; accounts: ProviderAccount[]; saving: boolean; onSave: (agent: CustomAgent) => Promise<boolean>; onClose: () => void; creating: boolean }) {
  const [agent, setAgent] = useState(initial);
  const [discard, setDiscard] = useState(false);
  const dirty = JSON.stringify(agent) !== JSON.stringify(initial);
  const problem = agent.model ? modelProblem(agent.model, accounts) : null;
  useModelProblemNotice("Agente customizado", problem);
  const close = () => { if (saving) return; if (dirty) setDiscard(true); else onClose(); };
  const patch = (value: Partial<CustomAgent>) => setAgent(current => ({ ...current, ...value }));
  return <><Dialog open onOpenChange={open => { if (!open) close(); }}>
    <DialogContent className="dark flex max-h-[92dvh] w-[calc(100vw-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[1000px]">
      <DialogHeader className="shrink-0 border-b border-border px-5 py-4">
        <DialogTitle>{creating ? "Adicionar agente" : "Editar agente"}</DialogTitle>
        <DialogDescription>Defina a identidade, as instruções e as ferramentas deste agente.</DialogDescription>
      </DialogHeader>
      <form className="flex min-h-0 flex-col" onSubmit={event => { event.preventDefault(); if (problem || saving) return; void onSave({ ...agent, name: agent.name.trim() }).then(saved => { if (saved) onClose(); }); }}>
        <div className="min-h-0 overflow-y-auto">
          <div className="grid min-w-0 gap-6 p-5 md:grid-cols-[320px_minmax(0,1fr)]">
            <fieldset disabled={saving} className="min-w-0 space-y-3">
              <legend className="sr-only">Identidade e permissões</legend>
              <div className="space-y-1.5"><Label htmlFor="custom-agent-name">Nome</Label><Input id="custom-agent-name" value={agent.name} maxLength={100} required onChange={e => patch({ name: e.target.value })} /></div>
              <div className="space-y-1.5"><Label htmlFor="custom-agent-description">Descrição</Label><Textarea id="custom-agent-description" className="min-h-16 resize-y text-xs" value={agent.description} maxLength={500} onChange={e => patch({ description: e.target.value })} /></div>
              <AppearancePicker value={agent.appearance ?? agentAppearance} onChange={appearance => patch({ appearance })} disabled={saving} />
              <div className="space-y-2">
      <ChoiceField label="Permissões" value={agent.capability} onChange={value => patch({ capability: value as CustomAgent["capability"] })} options={Object.entries(CAPABILITY_LABELS).map(([value, label]) => ({ value, label }))} />
                <p className="text-xs leading-5 text-muted-foreground">{agent.capability === "read_only" ? "Consulta o projeto sem editar arquivos ou executar comandos." : agent.capability === "write_files" ? "Pode ler e editar arquivos. Comandos e ferramentas MCP ficam indisponíveis." : "Pode editar arquivos, executar comandos e usar MCPs disponíveis. As aprovações da conversa continuam valendo."}</p>
              </div>
            </fieldset>
            <fieldset disabled={saving} className="flex min-w-0 flex-col gap-4">
              <legend className="sr-only">Comportamento e modelo</legend>
              <div className="flex min-h-60 flex-1 flex-col gap-2"><Label htmlFor="custom-agent-instructions">Instruções do agente</Label><Textarea id="custom-agent-instructions" className="min-h-60 flex-1 resize-y font-mono text-xs leading-5" value={agent.instructions} maxLength={16000} required placeholder="Descreva a especialidade, o objetivo e como o agente deve trabalhar." onChange={e => patch({ instructions: e.target.value })} /><p className="text-xs text-muted-foreground">Explique o objetivo, os limites e o resultado esperado.</p></div>
      <div className="space-y-2 rounded-md border border-border bg-sidebar p-3"><p className="text-xs font-medium">Modelo</p><div className="flex flex-wrap items-center gap-2"><ModelPicker modelGroups={accountGroups(accounts)} selection={agent.model ? { model: `${agent.model.account}/${agent.model.model}`, reasoning: agent.model.reasoning } : null} ariaLabel="Modelo do agente customizado" onSelect={next => { const split = next.model.indexOf("/"); patch({ model: { account: next.model.slice(0, split), model: next.model.slice(split + 1), reasoning: next.reasoning } }); }} /><Button type="button" variant="outline" size="sm" className="cursor-pointer text-xs" onClick={() => patch({ model: null })}>Usar modelo do chat</Button></div><p className="text-xs text-muted-foreground">{agent.model ? `Modelo fixo: ${agent.model.account}/${agent.model.model}` : "Este agente usa o modelo selecionado no composer."}</p></div>
              {problem && <p role="alert" className="text-xs text-destructive">{problem}</p>}
            </fieldset>
          </div>
        </div>
        <DialogFooter className="m-0 shrink-0 gap-3 border-t border-border bg-sidebar/50 px-5 py-4">
          <Button type="button" variant="outline" className="cursor-pointer" disabled={saving} onClick={close}>Cancelar</Button>
          <Button type="submit" className="cursor-pointer" disabled={saving || Boolean(problem) || !agent.name.trim() || !agent.instructions.trim()}>{saving ? "Salvando…" : "Salvar agente"}</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog><DiscardDialog open={discard} onOpenChange={setDiscard} onDiscard={onClose} /></>;
}
