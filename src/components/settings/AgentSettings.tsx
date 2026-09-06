import { CircleHelp } from "lucide-react";
import { AGENT_DESCRIPTIONS, AGENT_ICONS } from "@/components/agents/agent-presentation";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { Skeleton } from "@/components/ui/skeleton";
import { ModelPicker } from "@/components/chat/ModelPicker";
import { useAgentModels } from "@/hooks/use-agent-models";
import { FLOW_LABELS, ROLE_LABELS, ROLE_COLORS, type Workflow, type WorkflowAgent } from "@/core/workflow";
import type { ProviderAccount } from "@/core/provider-accounts";

type Role = WorkflowAgent["role"];
const GROUPS: { flow: Workflow; roles: Role[] }[] = [
  { flow: "standard", roles: ["builder"] }, { flow: "planned", roles: ["planner", "builder", "designer"] },
  { flow: "complete", roles: ["planner", "investigator", "writer", "orchestrator", "designer", "builder", "reviewer"] },
];
const GUIDANCE: Record<Role, string> = {
  planner: "Priorize análise e decisões de arquitetura. Exemplo: GPT 5.6 Sol com raciocínio Extra alto; Terra com Alto para planos menores.",
  investigator: "Priorize leitura eficiente e síntese com evidências. Exemplo: GPT 5.6 Luna com Alto; Sol com Alto para investigar problemas difíceis.",
  writer: "Busque clareza e consistência ao escrever tarefas e critérios de aceite. Exemplo: GPT 5.6 Luna com Médio ou Terra com Alto para especificações complexas.",
  orchestrator: "Priorize seguir contratos, dependências e decisões entre agentes. Exemplo: GPT 5.6 Sol com Alto; Extra alto para fluxos complexos.",
  designer: "Prefira um modelo capaz de analisar referências visuais e implementar interfaces. Exemplo: GPT 5.6 Sol com Alto ou Terra com Alto. A inspeção visual também depende das ferramentas disponíveis.",
  builder: "Equilibre capacidade de programação e volume de trabalho. Exemplo: GPT 5.6 Terra com Alto; Sol com Extra alto para alterações complexas; Luna com Alto para tarefas menores.",
  reviewer: "Priorize análise crítica independente. Exemplo: GPT 5.6 Sol com Extra alto. Usar um modelo diferente do Construtor pode trazer outra perspectiva, sem garantir a detecção de todos os problemas.",
};
export function AgentSettings({ accounts }: { accounts: ProviderAccount[] }) {
  const models = useAgentModels();
  const groups = accounts.filter(account => account.enabled && account.modelsAvailable).map(account => ({ provider: account.alias, models: account.models.map(model => ({ value: `${account.alias}/${model.id}`, label: model.name, reasoningLevels: model.reasoningLevels, defaultReasoningLevel: model.defaultReasoningLevel })) }));
  if (models.error) return <div role="alert" className="space-y-3 text-xs"><p className="text-destructive">{models.error}</p><Button variant="outline" className="cursor-pointer" onClick={() => void models.refresh()}>Tentar novamente</Button></div>;
  if (!models.data) return <div role="status" aria-label="Carregando modelos dos agentes" className="grid grid-cols-2 gap-3"><Skeleton className="h-24" /><Skeleton className="h-24" /><Skeleton className="h-24" /><Skeleton className="h-24" /></div>;
  return <TooltipProvider delay={150}><div className="space-y-6">{GROUPS.map(({ flow, roles }) => <section key={flow} aria-label={`Agentes do fluxo ${FLOW_LABELS[flow]}`}>
    <h3 className="micro-label mb-3 text-muted-foreground">{FLOW_LABELS[flow]}</h3>
    <div className="grid gap-3 sm:grid-cols-2">{roles.map(role => {
      const choice = models.data?.[`${flow}/${role}`];
      const Icon = AGENT_ICONS[role];
      return <Card key={role} size="sm" className="instrument-panel min-w-0 gap-0 overflow-hidden border border-border bg-card/60">
        <CardHeader className="flex flex-row items-center justify-between gap-2 pb-3"><div className="flex min-w-0 items-center gap-2.5"><span aria-hidden="true" className="flex size-8 shrink-0 items-center justify-center rounded-md border" style={{ color: ROLE_COLORS[role], borderColor: `${ROLE_COLORS[role]}35`, backgroundColor: `${ROLE_COLORS[role]}10` }}><Icon className="size-4" /></span><CardTitle className="text-xs" style={{ color: ROLE_COLORS[role] }}>{ROLE_LABELS[role]}</CardTitle></div>
          <Tooltip><TooltipTrigger render={<Button variant="ghost" size="icon" />} aria-label={`Como escolher o modelo de ${ROLE_LABELS[role]} no fluxo ${FLOW_LABELS[flow]}`} className="size-6 cursor-pointer text-muted-foreground"><CircleHelp className="size-3.5" /></TooltipTrigger><TooltipContent side="left" className="max-w-80 border border-border bg-card p-3 text-xs leading-5 text-foreground">{GUIDANCE[role]}</TooltipContent></Tooltip>
        </CardHeader>
        <CardContent className="flex min-w-0 flex-1 flex-col gap-4 overflow-hidden"><p className="min-h-[4.5em] text-xs leading-5 text-muted-foreground">{AGENT_DESCRIPTIONS[role]}</p><div className="mt-auto border-t border-border pt-2"><ModelPicker modelGroups={groups} selection={choice ? { model: `${choice.account}/${choice.model}`, reasoning: choice.reasoning } : null} disabled={models.saving} ariaLabel={`Modelo de ${ROLE_LABELS[role]} no fluxo ${FLOW_LABELS[flow]}`} onSelect={next => { const split = next.model.indexOf("/"); void models.save(flow, role, { account: next.model.slice(0, split), model: next.model.slice(split + 1), reasoning: next.reasoning }); }} /></div></CardContent>
      </Card>;
    })}</div>
  </section>)}</div></TooltipProvider>;
}
