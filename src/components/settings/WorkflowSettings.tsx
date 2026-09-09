import { useState } from "react";
import { Bot, Copy, LockKeyhole, Plus, Route, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogTrigger } from "@/components/ui/dialog";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "@/components/ui/alert-dialog";
import { BUILTIN_FLOWS } from "@/components/agents/workflow-presentation";
import { AGENT_DESCRIPTIONS, AGENT_ICONS } from "@/components/agents/agent-presentation";
import { ROLE_LABELS } from "@/core/workflow";
import { AGENT_USAGE_LABELS, CAPABILITY_LABELS, customId, type BuiltinFlow, type CustomAgent, type CustomFlow } from "@/core/workflow-catalog";
import type { ProviderAccount } from "@/core/provider-accounts";
import { useWorkflowCatalog } from "@/hooks/use-workflow-catalog";
import { AgentSettings } from "./AgentSettings";
import { AgentInstructionsDialog } from "./AgentInstructionsDialog";
import { CustomAgentEditor } from "./workflow/CustomAgentEditor";
import { CustomFlowEditor } from "./workflow/CustomFlowEditor";
import { WorkflowIdentityIcon } from "@/components/agents/WorkflowIdentityIcon";
import { agentAppearance, flowAppearance } from "@/core/workflow-appearance";

type Editor = { kind: "agent"; value: CustomAgent; revision: number; creating: boolean } | { kind: "flow"; value: CustomFlow; revision: number; creating: boolean };
const roles = ["planner", "investigator", "writer", "orchestrator", "designer", "builder", "reviewer"] as const;
const mutedCard = "instrument-panel flex w-full cursor-pointer items-center gap-3 rounded-md border border-border bg-sidebar/50 p-3 text-left text-muted-foreground hover:border-primary/40 hover:bg-card focus-visible:ring-2 focus-visible:ring-ring";

export function WorkflowSettings({ accounts }: { accounts: ProviderAccount[] }) {
  const catalog = useWorkflowCatalog();
  const [tab, setTab] = useState("flows");
  const [builtin, setBuiltin] = useState<BuiltinFlow | null>(null);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [deleting, setDeleting] = useState<{ kind: "agent" | "flow"; id: string; name: string; revision: number } | null>(null);
  const createAgent = () => { if (catalog.data) setEditor({ kind: "agent", creating: true, revision: catalog.data.revision, value: { id: customId(), name: "", description: "", instructions: "", usage: "mixed", capability: "read_only", model: null } }); };
  const createFlow = () => { if (catalog.data) setEditor({ kind: "flow", creating: true, revision: catalog.data.revision, value: { id: customId(), name: "", description: "", entry: "", maxSteps: 24, steps: [] } }); };
  const customSection = (kind: "agent" | "flow") => <section aria-label={kind === "flow" ? "Fluxos customizados" : "Agentes customizados"} className="space-y-3">
    <div className="flex items-center justify-between gap-3"><h3 className="micro-label text-muted-foreground">Customizados</h3><Button variant="outline" size="sm" className="cursor-pointer text-xs" disabled={!catalog.data || catalog.saving} onClick={kind === "flow" ? createFlow : createAgent}><Plus />{kind === "flow" ? "Adicionar fluxo" : "Adicionar agente"}</Button></div>
    {catalog.error ? <div role="alert" className="space-y-2 text-xs text-destructive"><p>{catalog.error}</p><Button variant="outline" size="sm" className="cursor-pointer" onClick={() => void catalog.refresh()}>Tentar novamente</Button></div> : !catalog.data ? <div role="status" aria-label="Carregando customizados" className="space-y-2"><Skeleton className="h-20" /><Skeleton className="h-20" /></div> : <>
      {(kind === "flow" ? catalog.data.flows : catalog.data.agents).map(item => <Card key={item.id} className="instrument-panel flex-row items-center gap-1 p-2">
        <Button variant="ghost" className="h-auto min-w-0 flex-1 cursor-pointer justify-start gap-3 px-2 py-3 text-left whitespace-normal" onClick={() => { const revision = catalog.data?.revision ?? 0; setEditor(kind === "flow" ? { kind, value: item as CustomFlow, revision, creating: false } : { kind, value: item as CustomAgent, revision, creating: false }); }} aria-label={`Editar ${item.name}`}>
          <WorkflowIdentityIcon appearance={item.appearance} fallback={kind === "flow" ? flowAppearance : agentAppearance} className="size-5 shrink-0" /><span className="min-w-0 flex-1"><span className="block text-xs font-medium">{item.name}</span><span className="mt-1 block text-[11px] leading-4 text-muted-foreground">{item.description || (kind === "flow" ? "Fluxo personalizado" : "Agente personalizado")}</span><span className="mt-2 block font-mono text-[10px] text-muted-foreground">{"steps" in item ? `${item.steps.length} etapas` : `${AGENT_USAGE_LABELS[item.usage]} · ${CAPABILITY_LABELS[item.capability]}`}</span></span>
        </Button><Button variant="ghost" size="icon" className="size-7 cursor-pointer text-muted-foreground" aria-label={`Duplicar ${item.name}`} onClick={() => { const copy = { ...item, id: customId(), name: `${item.name.slice(0, 85)} (cópia)` }; const revision = catalog.data?.revision ?? 0; setEditor(kind === "flow" ? { kind, value: copy as CustomFlow, revision, creating: true } : { kind, value: copy as CustomAgent, revision, creating: true }); }}><Copy className="size-3.5" /></Button><Button variant="ghost" size="icon" className="size-7 cursor-pointer text-muted-foreground hover:text-destructive" aria-label={`Excluir ${item.name}`} onClick={() => setDeleting({ kind, id: item.id, name: item.name, revision: catalog.data?.revision ?? 0 })}><Trash2 className="size-3.5" /></Button>
      </Card>)}
      {(kind === "flow" ? catalog.data.flows : catalog.data.agents).length === 0 && <p className="rounded-md border border-dashed border-border p-5 text-center text-xs text-muted-foreground">{kind === "flow" ? "Crie um fluxo e conecte seus agentes no canvas." : "Crie agentes com suas próprias instruções e permissões."}</p>}
    </>}
  </section>;
  return <>
    <Tabs value={tab} onValueChange={setTab} className="gap-5"><TabsList aria-label="Workflow" aria-orientation="horizontal" className="h-9 flex-row! items-center bg-sidebar"><TabsTrigger value="flows" className="w-auto! flex-none! cursor-pointer justify-center! gap-2 px-4 text-xs"><Route />Fluxos</TabsTrigger><TabsTrigger value="agents" className="w-auto! flex-none! cursor-pointer justify-center! gap-2 px-4 text-xs"><Bot />Agentes</TabsTrigger></TabsList>
      <TabsContent value="flows" className="space-y-6"><section aria-label="Fluxos Jarvis" className="space-y-3"><h3 className="micro-label text-muted-foreground">Jarvis</h3><p className="text-xs text-muted-foreground">Fluxos do Jarvis · organização e instruções fixas.</p>{BUILTIN_FLOWS.map(flow => <Button key={flow.value} variant="ghost" className={`h-auto whitespace-normal ${mutedCard}`} aria-label={`Ver fluxo ${flow.title}`} onClick={() => setBuiltin(flow.value)}><flow.icon className="size-5 shrink-0 opacity-65" /><span className="min-w-0 flex-1"><span className="block text-xs font-medium">{flow.title}</span><span className="mt-1 block text-[11px] leading-4">{flow.description}</span></span><LockKeyhole className="size-3.5 shrink-0" /></Button>)}</section>{customSection("flow")}</TabsContent>
      <TabsContent value="agents" className="space-y-6"><section aria-label="Agentes Jarvis" className="space-y-3"><h3 className="micro-label text-muted-foreground">Jarvis</h3><p className="text-xs text-muted-foreground">Agentes do Jarvis · clique para consultar as instruções.</p>{roles.map(role => { const Icon = AGENT_ICONS[role]; return <AgentInstructionsDialog key={role} flow="complete" role={role}><DialogTrigger render={<Button variant="ghost" />} className={`h-auto whitespace-normal ${mutedCard}`} aria-label={`Ver agente ${ROLE_LABELS[role]}`}><Icon className="size-5 shrink-0 opacity-65" /><span className="min-w-0 flex-1"><span className="block text-xs font-medium">{ROLE_LABELS[role]}</span><span className="mt-1 block text-[11px] leading-4">{AGENT_DESCRIPTIONS[role]}</span></span><LockKeyhole className="size-3.5 shrink-0" /></DialogTrigger></AgentInstructionsDialog>; })}</section>{customSection("agent")}</TabsContent>
    </Tabs>
    <Dialog open={builtin !== null} onOpenChange={open => { if (!open) setBuiltin(null); }}><DialogContent className="dark max-h-[85dvh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle className="flex items-center gap-2">{BUILTIN_FLOWS.find(flow => flow.value === builtin)?.title}<Badge variant="outline" className="text-[10px]"><LockKeyhole className="mr-1 size-3" />Jarvis</Badge></DialogTitle><DialogDescription>Organização e instruções protegidas. A escolha de modelos continua disponível para executar este fluxo.</DialogDescription></DialogHeader>{builtin && <AgentSettings accounts={accounts} flowFilter={builtin} />}</DialogContent></Dialog>
    {editor?.kind === "agent" && <CustomAgentEditor initial={editor.value} creating={editor.creating} accounts={accounts} saving={catalog.saving} onClose={() => setEditor(null)} onSave={async agent => { const saved = await catalog.mutate({ kind: "save_agent", agent }, editor.revision); if (saved) toast.success("Agente salvo."); return saved; }} />}
    {editor?.kind === "flow" && <CustomFlowEditor initial={editor.value} creating={editor.creating} agents={catalog.data?.agents.filter(agent => agent.usage !== "solo") ?? []} saving={catalog.saving} onClose={() => setEditor(null)} onSave={async flow => { const saved = await catalog.mutate({ kind: "save_flow", flow }, editor.revision); if (saved) toast.success("Fluxo salvo."); return saved; }} />}
    <AlertDialog open={deleting !== null} onOpenChange={open => { if (!open && !catalog.saving) setDeleting(null); }}><AlertDialogContent className="dark"><AlertDialogHeader><AlertDialogTitle>Excluir {deleting?.name}?</AlertDialogTitle><AlertDialogDescription>{deleting?.kind === "agent" ? "Agentes vinculados a fluxos precisam ser desvinculados antes da exclusão." : "O fluxo deixa de aparecer no composer. Execuções já iniciadas preservam sua definição."}</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={catalog.saving}>Cancelar</AlertDialogCancel><AlertDialogAction variant="destructive" className="cursor-pointer" disabled={catalog.saving} onClick={() => { if (deleting) void catalog.mutate({ kind: deleting.kind === "agent" ? "delete_agent" : "delete_flow", id: deleting.id }, deleting.revision).then(saved => { if (saved) { setDeleting(null); toast.success("Excluído."); } }); }}>Excluir</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
  </>;
}
