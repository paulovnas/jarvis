import { lazy, Suspense, useState } from "react";
import { LockKeyhole, Network } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { WorkflowIdentityIcon } from "@/components/agents/WorkflowIdentityIcon";
import type { ProviderAccount } from "@/core/provider-accounts";
import type { BuiltinAgentDefinition, BuiltinFlowDefinition } from "@/core/workflow-catalog";
import { AgentInstructionsDialog } from "../AgentInstructionsDialog";
import { AgentSettings } from "../AgentSettings";

const Canvas = lazy(() => import("./WorkflowCanvas"));

export function BuiltinFlowDialog({ flow, agents, accounts, onClose }: { flow: BuiltinFlowDefinition; agents: BuiltinAgentDefinition[]; accounts: ProviderAccount[]; onClose: () => void }) {
  const [selected, setSelected] = useState<string | null>(flow.entry);
  const selectedStep = flow.steps.find(step => step.id === selected);
  const selectedAgent = agents.find(agent => agent.id === selectedStep?.agentId);
  return <Dialog open onOpenChange={open => { if (!open) onClose(); }}>
    <DialogContent className="dark flex max-h-[92dvh] w-[96vw] flex-col gap-0 overflow-y-auto p-0 sm:max-w-[1240px]">
      <DialogHeader className="shrink-0 border-b border-border bg-card px-5 py-4 pr-12">
        <DialogTitle className="flex items-center gap-2"><WorkflowIdentityIcon appearance={flow.appearance} className="size-5" />{flow.name}<Badge variant="outline" className="text-[10px] text-muted-foreground"><LockKeyhole className="mr-1 size-3" />Somente leitura</Badge></DialogTitle>
        <DialogDescription>{flow.description} O canvas abaixo representa a topologia executada pelo Jarvis.</DialogDescription>
      </DialogHeader>
      <div className="grid min-h-[470px] gap-3 p-4 lg:grid-cols-[minmax(0,1fr)_260px]">
        <Suspense fallback={<Skeleton className="min-h-[440px]" aria-label="Carregando canvas do fluxo" />}>
          <Canvas flow={flow} agents={agents} selected={selected} onSelect={setSelected} onChange={() => undefined} disabled />
        </Suspense>
        <aside className="space-y-3">
          <Card className="instrument-panel gap-0 py-0">
            <CardHeader className="border-b border-border p-4"><CardTitle className="micro-label flex items-center gap-2 text-muted-foreground"><Network className="size-3.5 text-onedark-cyan" />Topologia nativa</CardTitle></CardHeader>
            <CardContent className="space-y-3 p-4 text-xs leading-5 text-muted-foreground"><p>As linhas em ciano mostram as delegações que cada coordenador pode fazer. O agente escolhe apenas as rotas necessárias para o pedido atual.</p><p>Os mesmos agentes Jarvis podem ser usados como blocos imutáveis em fluxos personalizados.</p></CardContent>
          </Card>
          {selectedAgent && <Card className="instrument-panel gap-0 py-0">
            <CardHeader className="p-4 pb-2"><CardTitle className="flex items-center gap-2 text-sm"><WorkflowIdentityIcon appearance={selectedAgent.appearance} className="size-4" />{selectedAgent.name}</CardTitle></CardHeader>
            <CardContent className="space-y-3 p-4 pt-1"><p className="text-xs leading-5 text-muted-foreground">{selectedAgent.description}</p><AgentInstructionsDialog flow={flow.id} role={selectedAgent.role}><DialogTrigger render={<Button variant="outline" size="sm" />} className="w-full cursor-pointer text-xs"><LockKeyhole />Ver instruções</DialogTrigger></AgentInstructionsDialog></CardContent>
          </Card>}
        </aside>
      </div>
      <section aria-label={`Modelos do fluxo ${flow.name}`} className="border-t border-border bg-sidebar/40 p-5"><h3 className="micro-label mb-3 text-muted-foreground">Modelos deste fluxo</h3><AgentSettings accounts={accounts} flowFilter={flow.id} /></section>
    </DialogContent>
  </Dialog>;
}
