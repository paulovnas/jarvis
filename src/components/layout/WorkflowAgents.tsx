import { useEffect, useRef, useState } from "react";
import { ChevronRight, CircleAlert } from "lucide-react";
import { AGENT_ICONS } from "@/components/agents/agent-presentation";
import { aliasSuffix } from "@/core/provider-usage";
import { reasoningLabel } from "@/core/reasoning";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { activeAgent, ROLE_COLORS, ROLE_LABELS, STATUS_LABELS, type WorkflowAgent } from "@/core/workflow";
import { useWorkflowTranscript, type WorkflowController } from "@/hooks/use-workflow";
import { TurnBody } from "@/components/chat/Transcript";
import { UserMessageBubble } from "@/components/chat/UserMessageBubble";
import { CompactionMarker } from "@/components/chat/CompactionMarker";
import { workflowAppearance } from "@/components/agents/workflow-appearance";
import { agentAppearance, flowAppearance } from "@/core/workflow-appearance";
import { reasoningPreview } from "@/components/chat/reasoning-preview";
import { executionDuration, formatExecutionDuration, useRunningClock } from "@/hooks/use-running-clock";

function presentation(agent: WorkflowAgent) {
  return agent.role === "custom"
    ? { ...workflowAppearance(agent.identity?.appearance, agent.id === "main" ? flowAppearance : agentAppearance), label: agent.identity?.name ?? ROLE_LABELS.custom }
    : { Icon: AGENT_ICONS[agent.role], color: ROLE_COLORS[agent.role], label: ROLE_LABELS[agent.role] };
}

function ModelDetails({ agent }: { agent: WorkflowAgent }) {
  return <span className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1 font-mono text-[9px] text-muted-foreground" title={`${agent.options.account} / ${agent.options.model}`}>
    <span className="max-w-24 truncate text-foreground/75">{aliasSuffix(agent.options.account)}</span><span aria-hidden="true" className="text-border">/</span><span className="truncate">{agent.options.model}</span>
    {agent.options.reasoning && <><span aria-hidden="true">·</span><span>{reasoningLabel(agent.options.reasoning)}</span></>}
  </span>;
}
function StatusBadge({ agent }: { agent: WorkflowAgent }) {
  const color = agent.status === "completed" ? "var(--color-onedark-green)" : ["failed", "blocked"].includes(agent.status) ? "var(--destructive)" : activeAgent(agent) ? presentation(agent).color : "var(--muted-foreground)";
  return <Badge variant="outline" className="text-[9px]" style={{ color, borderColor: `color-mix(in srgb, ${color} 25%, transparent)`, backgroundColor: `color-mix(in srgb, ${color} 6%, transparent)` }}>{STATUS_LABELS[agent.status]}</Badge>;
}
const timingActive = (agent: WorkflowAgent) => agent.status === "running" || agent.status === "waiting";
function AgentHistory({ conversationId, agent }: { conversationId: string; agent: WorkflowAgent }) {
  const transcript = useWorkflowTranscript(conversationId, agent.id);
  const root = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  useEffect(() => {
    const viewport = root.current?.querySelector<HTMLElement>('[data-slot="scroll-area-viewport"]');
    if (viewport && follow.current) viewport.scrollTop = viewport.scrollHeight;
  }, [transcript.data]);
  return <div ref={root} className="flex min-h-0 flex-1 flex-col">
    {transcript.error ? <div role="alert" className="space-y-3 p-5 text-xs"><p className="text-destructive">{transcript.error}</p><Button variant="outline" size="sm" className="cursor-pointer" onClick={transcript.retry}>Tentar novamente</Button></div>
      : !transcript.data ? <div role="status" aria-label="Carregando histórico do agente" className="space-y-5 p-6"><Skeleton className="ml-auto h-16 w-2/3" /><Skeleton className="h-6 w-1/3" /><Skeleton className="h-32 w-full" /></div>
      : <ScrollArea className="min-h-0 flex-1" onScrollCapture={event => { const view = event.target; if (view instanceof HTMLElement && view.dataset.slot === "scroll-area-viewport") follow.current = view.scrollHeight - view.scrollTop - view.clientHeight < 100; }}>
        <div aria-label="Histórico do agente" className="px-5 pb-5">{transcript.data.turns.map(turn => <div key={turn.id}>
          <UserMessageBubble message={{ id: turn.id, role: "user", content: turn.user, timestamp: new Date(turn.createdAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" }) }} />
          {transcript.data?.compactions?.filter(event => event.turnId === turn.id && !event.afterTurn).map(event => <CompactionMarker key={event.id} event={event} />)}
          <TurnBody turn={turn} />
          {transcript.data?.compactions?.filter(event => event.turnId === turn.id && event.afterTurn).map(event => <CompactionMarker key={event.id} event={event} />)}
        </div>)}</div>
      </ScrollArea>}
  </div>;
}
export function WorkflowAgents({ workflow, conversationId }: { workflow?: WorkflowController; conversationId?: string }) {
  const [selected, setSelected] = useState<string | null>(null);
  const agents = (workflow?.data?.agents ?? []).filter(agent => workflow?.data?.flow !== "publication" || agent.id !== "main");
  const now = useRunningClock(agents.some(timingActive));
  const latestByRole = new Map<WorkflowAgent["role"], WorkflowAgent>();
  const currentAgents = agents.filter(agent => {
    if (agent.id === "main" || agent.role === "custom") return true;
    const current = latestByRole.get(agent.role);
    if (!current || agent.createdAt > current.createdAt || agent.createdAt === current.createdAt && (agent.updatedAt > current.updatedAt || agent.updatedAt === current.updatedAt && agent.id > current.id)) {
      latestByRole.set(agent.role, agent);
    }
    return false;
  }).concat([...latestByRole.values()]);
  const selectedAgent = currentAgents.find(agent => agent.id === selected);
  const SelectedIcon = selectedAgent ? presentation(selectedAgent).Icon : AGENT_ICONS.custom;
  const ordered = [...currentAgents].sort((a, b) => Number(activeAgent(b)) - Number(activeAgent(a)) || a.createdAt - b.createdAt || a.updatedAt - b.updatedAt || a.id.localeCompare(b.id));
  return <>
    {workflow?.error ? <div role="alert" className="space-y-2 text-xs"><p className="text-destructive">{workflow.error}</p><Button variant="ghost" size="sm" className="cursor-pointer" onClick={workflow.retry}>Tentar novamente</Button></div>
      : workflow?.loading ? <div role="status" aria-label="Carregando agentes" className="space-y-2"><Skeleton className="h-16 w-full" /><Skeleton className="h-16 w-full" /></div>
      : !currentAgents.length ? <p className="text-xs text-muted-foreground">Nenhum agente em execução.</p>
      : <div className="space-y-2">{ordered.map(agent => {
        const { Icon, color, label } = presentation(agent);
        const duration = executionDuration(agent.startedAt, agent.durationMs, timingActive(agent), now);
        const thought = agent.currentThought ? reasoningPreview(agent.currentThought) : "";
        const requiresAttention = Boolean(agent.pendingApproval || agent.pendingQuestion || agent.pendingAuthoring);
        return <Button key={agent.id} variant="ghost" data-status={agent.status} style={agent.status === "waiting" || agent.status === "queued" ? { borderColor: agent.role === "custom" ? `color-mix(in srgb, ${color} 50%, transparent)` : `${color}80` } : undefined} onClick={() => setSelected(agent.id)} aria-label={`Abrir agente ${label}: ${agent.title}`} className="agent-execution relative isolate h-auto w-full cursor-pointer flex-col items-stretch gap-2 rounded-md border border-border bg-card/60 p-3 text-left whitespace-normal shadow-[inset_0_1px_0_#ffffff0a] hover:border-primary/40">
          <span className="flex items-center gap-2"><Icon aria-hidden="true" className="size-3.5 shrink-0" style={{ color }} /><span className="flex-1 text-xs font-medium">{label}</span>{requiresAttention && selected !== agent.id && <span aria-label="Aguardando sua resposta" title="Aguardando sua resposta" className="flex size-5 items-center justify-center rounded-full border border-onedark-yellow/30 bg-onedark-yellow/10 text-onedark-yellow"><CircleAlert aria-hidden="true" className="size-3" /></span>}{agent.status === "running" && <span aria-label="Em execução" className="size-1.5 animate-pulse rounded-full motion-reduce:animate-none" style={{ backgroundColor: color }} />}<ChevronRight className="size-3 text-muted-foreground" /></span>
          {agent.title !== label && <span className="line-clamp-2 text-[11px] leading-4 text-muted-foreground">{agent.title}</span>}
          {thought && <span title={thought} className={`line-clamp-2 text-[10px] italic leading-4 text-muted-foreground ${agent.status === "running" ? "reasoning-shimmer" : ""}`}>{thought}</span>}
          <span className="flex items-center gap-2"><StatusBadge agent={agent} /><span aria-label="Tempo de execução" className="font-mono text-[9px] tabular-nums text-muted-foreground">{formatExecutionDuration(duration)}</span></span>
          <ModelDetails agent={agent} />
        </Button>;
      })}</div>}
    <Dialog open={!!selectedAgent} onOpenChange={open => { if (!open) setSelected(null); }}>
      {selectedAgent && conversationId && <DialogContent className="dark flex h-[min(760px,85dvh)] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl">
        <DialogHeader className="shrink-0 border-b border-border bg-sidebar p-5 pr-12">
          <DialogDescription className="micro-label flex items-center gap-2" style={{ color: presentation(selectedAgent).color }}><SelectedIcon className="size-3.5" />{presentation(selectedAgent).label}<StatusBadge agent={selectedAgent} /></DialogDescription>
          <DialogTitle className="text-sm">{selectedAgent.title}</DialogTitle>
          <ModelDetails agent={selectedAgent} />
          {selectedAgent.attempts > 1 && <span className="font-mono text-[10px] text-muted-foreground">Rodada {selectedAgent.attempts}</span>}
        </DialogHeader>
        <AgentHistory key={`${conversationId}/${selectedAgent.id}`} conversationId={conversationId} agent={selectedAgent} />
      </DialogContent>}
    </Dialog>
  </>;
}
