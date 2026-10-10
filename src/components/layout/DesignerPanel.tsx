import { useEffect, useRef, useState } from "react";
import { Check, ChevronRight, CirclePause, Palette, Pencil, Play, Square, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Field, FieldLabel } from "@/components/ui/field";
import { Hint } from "@/components/ui/hint";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import type { ImpeccableLiveController, ImpeccableLiveTask } from "@/hooks/use-impeccable-live";

export type DesignerSession = {
  conversationId: string;
  live: ImpeccableLiveController;
  canResume: boolean;
  attention: boolean;
  onResume: () => Promise<void>;
  onShowChat: () => void;
};
const phaseLabels = { idle: "Pronto", connecting: "Conectando", waiting: "Aguardando agente", running: "Em andamento", blocked: "Precisa de atenção" };
const taskLabels = { pending: "Pendente", paused: "Pausada", running: "Em andamento", completed: "Concluída", cancelled: "Cancelada", blocked: "Precisa de atenção" };
const editable = (task: ImpeccableLiveTask) => task.status === "pending" || task.status === "paused";

export function DesignerPanel({ session }: { session?: DesignerSession }) {
  const [editing, setEditing] = useState<{ task: ImpeccableLiveTask; content: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [acting, setActing] = useState<string | null>(null);
  const live = session?.live;
  const status = live?.status;
  const tasks = status?.tasks ?? [];
  const currentTasks = useRef(tasks);
  useEffect(() => { currentTasks.current = tasks; });
  const pending = tasks.filter(task => editable(task));
  const running = tasks.filter(task => task.status === "running" || task.status === "blocked");
  const finished = tasks.filter(task => task.status === "completed" || task.status === "cancelled").slice(-6).reverse();
  const locked = !!acting || !!live?.busy;
  const perform = async (id: string, action: () => Promise<string | null>, success?: string) => {
    if (locked) return;
    setActing(id); setError(null);
    try {
      const result = await action();
      if (result) setError(result);
      else if (success) toast.success(success);
      return result;
    } finally { setActing(null); }
  };
  const renderTask = (task: ImpeccableLiveTask) => {
    const isEditing = editing?.task.requestId === task.requestId;
    return <li key={task.requestId} className="border-b border-border/60 py-3 last:border-0" aria-label={`Alteração: ${task.content}`}>
      <div className="flex items-start gap-2">
        <p className="min-w-0 flex-1 whitespace-pre-wrap break-words text-xs leading-relaxed text-foreground">{task.content}</p>
        <div className="flex shrink-0 gap-0.5">
          {(editable(task) || task.status === "blocked") && <>
            {(task.status === "paused" || task.status === "blocked") && <Hint content="Retomar com o modelo selecionado no chat"><Button type="button" variant="ghost" size="icon" className="size-7 cursor-pointer text-onedark-cyan" aria-label={`Retomar alteração: ${task.content}`} disabled={locked || !session?.canResume} onClick={() => { if (live) void perform(task.requestId, () => live.resumeTask(task)); }}><Play aria-hidden="true" className="size-3.5" /></Button></Hint>}
            {editable(task) && <Hint content="Editar pedido"><Button type="button" variant="ghost" size="icon" className="size-7 cursor-pointer text-muted-foreground" aria-label={`Editar alteração: ${task.content}`} disabled={locked} onClick={() => { setError(null); setEditing({ task, content: task.content }); }}><Pencil aria-hidden="true" className="size-3.5" /></Button></Hint>}
            <Hint content="Remover da fila"><Button type="button" variant="ghost" size="icon" className="size-7 cursor-pointer text-muted-foreground hover:text-destructive" aria-label={`Remover alteração: ${task.content}`} disabled={locked} onClick={() => { if (live) void perform(task.requestId, () => live.removeTask(task), "Alteração removida da fila."); }}><Trash2 aria-hidden="true" className="size-3.5" /></Button></Hint>
          </>}
          {task.status === "running" && <Hint content="Interromper esta alteração"><Button type="button" variant="ghost" size="icon" className="size-7 cursor-pointer text-destructive" aria-label={`Cancelar alteração: ${task.content}`} disabled={locked} onClick={() => { if (live) void perform(task.requestId, () => live.cancelAction(task.actionId, task.requestId, task.revision)); }}><Square aria-hidden="true" className="size-3.5" /></Button></Hint>}
        </div>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-[10px] text-muted-foreground"><span className={task.status === "running" ? "text-onedark-purple" : task.status === "paused" || task.status === "blocked" ? "text-onedark-yellow" : undefined}>{taskLabels[task.status]}</span>{task.beadId && <span className="font-mono">{task.beadId}</span>}</div>
      {isEditing && editing && <Field className="mt-3 gap-2">
        <FieldLabel htmlFor={`designer-edit-${task.requestId}`} className="text-xs">Pedido da alteração</FieldLabel>
        <Textarea id={`designer-edit-${task.requestId}`} value={editing.content} autoFocus disabled={locked} className="min-h-24 resize-y text-xs" onChange={event => setEditing({ ...editing, content: event.target.value })} />
        {!editable(task) && <p role="alert" className="text-xs text-onedark-yellow">A alteração já saiu da fila. Seu texto foi preservado; ela não pode mais ser editada.</p>}
        <div className="flex justify-end gap-1">
          <Button type="button" variant="ghost" size="sm" className="h-7 cursor-pointer px-2 text-xs" disabled={locked} onClick={() => setEditing(null)}><X aria-hidden="true" className="size-3" />Cancelar edição</Button>
          <Button type="button" size="sm" className="h-7 cursor-pointer px-2 text-xs" disabled={locked || !editable(task) || !editing.content.trim()} onClick={() => { if (live) void perform(task.requestId, () => live.editTask(editing.task, editing.content), "Pedido atualizado.").then(result => {
            if (result === null) setEditing(null);
            else if (result) setEditing(current => {
              const updated = currentTasks.current.find(item => item.requestId === current?.task.requestId);
              // Preserve the user's text after a conflict; another explicit save
              // uses the current revision without replaying the rejected write.
              return current && updated && editable(updated) && updated.revision > current.task.revision ? { ...current, task: updated } : current;
            });
          }); }}><Check aria-hidden="true" className="size-3" />Salvar</Button>
        </div>
      </Field>}
    </li>;
  };
  return <ScrollArea className="h-full"><section aria-label="Designer e Impeccable Live" className="px-3 pb-4">
    <div className="border-b border-border py-4">
      <div className="flex items-center gap-2"><Palette aria-hidden="true" className="size-4 text-onedark-purple" /><h2 className="text-sm font-medium">Impeccable Live</h2><Badge variant="outline" className="ml-auto text-[10px]">Nativo</Badge></div>
      <p className="mt-2 text-xs leading-relaxed text-muted-foreground">Edite elementos da página com o Designer. O Impeccable está incluído no Jarvis e usa o modelo selecionado no chat.</p>
      {!session ? <p className="mt-3 text-xs text-muted-foreground">Abra uma conversa para acompanhar as alterações.</p> : !live?.loaded ? <div role="status" aria-label="Carregando integração Designer" className="mt-3 space-y-2"><Skeleton className="h-4 w-2/3" /><Skeleton className="h-4 w-full" /></div> : status && <div role="status" aria-live="polite" aria-label="Estado do Designer" className="mt-3 space-y-2 text-xs">
        <p className={status.phase === "running" ? "text-onedark-purple" : status.error || status.phase === "blocked" ? "text-onedark-yellow" : "text-foreground"}>{status.state === "off" && !status.error ? "Live desligado" : phaseLabels[status.state === "starting" ? "connecting" : status.phase]}</p>
        {(status.provider || status.model) && <p className="break-words font-mono text-[11px] text-muted-foreground">{[status.provider, status.model].filter(Boolean).join(" · ")}</p>}
        {status.url && <Hint content={status.url} whenTruncated><p className="truncate font-mono text-[10px] text-muted-foreground">{status.url}</p></Hint>}
        {(status.step || status.elapsedMs) ? <p className="font-mono text-[10px] text-muted-foreground">{[status.step ? `Etapa ${status.step}` : null, status.elapsedMs ? `${Math.floor(status.elapsedMs / 1000)}s` : null].filter(Boolean).join(" · ")}</p> : null}
        {status.activity && <p className="leading-relaxed text-muted-foreground">{status.activity}</p>}
        {status.error && <p className="leading-relaxed text-onedark-yellow">{status.error}</p>}
        {(status.error || status.phase === "blocked" || status.state === "setup") && <Button type="button" variant="outline" size="sm" className="h-7 cursor-pointer text-xs" disabled={locked || !session.canResume} onClick={() => { void session.onResume(); }}>Retomar Live</Button>}
        {status.state === "off" && !status.error && <p className="text-muted-foreground">Ative o Live no botão ao lado do microfone.</p>}
      </div>}
      {session?.attention && <Alert className="mt-3 p-2" role="status" aria-label="Solicitação pendente do Designer"><AlertDescription className="space-y-2 text-xs"><p>Uma solicitação precisa da sua resposta para continuar.</p><Button type="button" variant="outline" size="sm" className="h-7 cursor-pointer text-xs" onClick={session.onShowChat}>Responder no chat</Button></AlertDescription></Alert>}
    </div>
    {error && <p role="alert" className="py-3 text-xs leading-relaxed text-destructive">{error}</p>}
    <section aria-label="Alterações em andamento" className="pt-4"><h3 className="flex items-center gap-2 text-xs font-medium">Em andamento <Badge variant="secondary" className="font-mono text-[10px]">{running.length}</Badge></h3>{running.length ? <ul>{running.map(renderTask)}</ul> : <p className="py-3 text-xs text-muted-foreground">Nenhuma alteração em andamento.</p>}</section>
    <section aria-label="Alterações pendentes" className="border-t border-border pt-4"><h3 className="flex items-center gap-2 text-xs font-medium">Pendentes <Badge variant="secondary" className="font-mono text-[10px]">{pending.length}</Badge>{pending.some(task => task.status === "paused") && <CirclePause aria-hidden="true" className="ml-auto size-3.5 text-onedark-yellow" />}</h3>{pending.length ? <ul>{pending.map(renderTask)}</ul> : <p className="py-3 text-xs text-muted-foreground">Nenhuma alteração na fila.</p>}</section>
    {!!finished.length && <Collapsible className="border-t border-border pt-2"><CollapsibleTrigger render={<Button type="button" variant="ghost" />} className="group h-8 w-full cursor-pointer justify-start px-0 text-xs text-muted-foreground">Recentes <Badge variant="secondary" className="font-mono text-[10px]">{finished.length}</Badge><ChevronRight aria-hidden="true" className="ml-auto size-3 group-aria-expanded:rotate-90" /></CollapsibleTrigger><CollapsibleContent><ul>{finished.map(renderTask)}</ul></CollapsibleContent></Collapsible>}
  </section></ScrollArea>;
}
