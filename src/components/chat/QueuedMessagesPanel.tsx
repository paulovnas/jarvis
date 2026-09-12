import { useRef, useState } from "react";
import { CornerDownRight, GripVertical, ListOrdered, Pencil, SendHorizontal, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import { SortableItem, SortableList } from "@/components/layout/SortableList";
import type { QueuedMessage } from "@/core/chat";
import { MessageContent } from "./MessageContent";
import { Hint } from "@/components/ui/hint";

interface QueuedMessagesPanelProps {
  messages: QueuedMessage[];
  running: boolean;
  compacting: boolean;
  onEdit?: (id: string) => Promise<void>;
  onDelete?: (id: string) => Promise<boolean>;
  onSendNow?: (id: string) => Promise<boolean>;
  onReorder?: (ids: string[]) => Promise<boolean>;
  onResume?: () => Promise<void>;
}

export function QueuedMessagesPanel({
  messages,
  running,
  compacting,
  onEdit,
  onDelete,
  onSendNow,
  onReorder,
  onResume,
}: QueuedMessagesPanelProps) {
  const locks = useRef(new Set<string>());
  const [busyIds, setBusyIds] = useState<ReadonlySet<string>>(() => new Set());
  const [reordering, setReordering] = useState(false);
  const [resuming, setResuming] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<QueuedMessage | null>(null);

  const withMessageLock = async (id: string, action: () => Promise<void>) => {
    if (locks.current.has(id)) return;
    locks.current.add(id);
    setBusyIds(new Set(locks.current));
    try {
      await action();
    } finally {
      locks.current.delete(id);
      setBusyIds(new Set(locks.current));
    }
  };

  const reorder = async (ids: string[]) => {
    if (!onReorder || reordering) return;
    setReordering(true);
    try {
      await onReorder(ids);
    } finally {
      setReordering(false);
    }
  };

  const confirmDelete = async () => {
    const target = deleteTarget;
    if (!target || !onDelete) return;
    await withMessageLock(target.id, async () => {
      if (await onDelete(target.id)) setDeleteTarget(null);
    });
  };

  const locked = compacting || reordering;
  const ids = messages.map(message => message.id);

  return <>
    <section aria-label="Mensagens agendadas" className="mx-3 overflow-hidden rounded-t-xl border border-b-0 border-border bg-card/95 shadow-[inset_0_1px_0_#ffffff0d]">
      <div className="flex min-h-8 items-center gap-2 border-b border-border/60 px-3 text-[11px] text-muted-foreground">
        <ListOrdered aria-hidden="true" className="size-3.5 text-onedark-cyan" />
        <span className="font-medium text-foreground/85">Mensagens agendadas</span>
        <span className="font-mono text-[10px]">{messages.length}</span>
        <span className="text-muted-foreground/55">·</span>
        <span>{running ? "após a resposta atual" : "fila pausada"}</span>
        {!running && <Button
          type="button"
          variant="ghost"
          size="sm"
          className="ml-auto h-6 cursor-pointer px-2 text-[11px]"
          disabled={resuming || compacting || !onResume}
          onClick={() => {
            setResuming(true);
            void onResume?.().finally(() => setResuming(false));
          }}
        >Continuar fila</Button>}
      </div>
      <div className="max-h-44 overflow-y-auto px-1.5">
        <SortableList ids={ids} onReorder={next => { void reorder(next); }}>
          {messages.map((message, index) => <SortableItem key={message.id} id={message.id} disabled={locked || messages.length < 2}>{sort => {
            const itemBusy = busyIds.has(message.id);
            return <div ref={sort.setNodeRef} style={sort.style} data-queued-message={message.id} className="group/queued flex min-h-10 items-center gap-1 border-b border-border/45 px-1 last:border-b-0">
              {messages.length > 1 ? <Hint content="Arraste para reordenar"><Button
                ref={sort.setActivatorNodeRef}
                {...sort.attributes}
                {...sort.listeners}
                type="button"
                variant="ghost"
                size="icon"
                aria-label={`Reordenar mensagem ${index + 1}`}
                disabled={locked || itemBusy}
                className="size-6 shrink-0 cursor-grab touch-none text-muted-foreground/55 hover:text-foreground active:cursor-grabbing"
              ><GripVertical aria-hidden="true" className="size-3.5" /></Button></Hint> : <CornerDownRight aria-hidden="true" className="mx-1 size-3 shrink-0 text-muted-foreground/45" />}
              <Hint content={message.content}><div className="min-w-0 flex-1 truncate text-xs text-foreground/85">
                <MessageContent content={message.content} parts={message.parts} />
              </div></Hint>
              <Hint content={running ? "Adicionar à execução atual sem interrompê-la" : "Disponível durante uma execução"}><Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 shrink-0 cursor-pointer gap-1.5 px-2 text-[11px] text-muted-foreground hover:text-onedark-cyan"
                aria-label={`Enviar mensagem ${index + 1} agora`}
                disabled={!running || locked || itemBusy || !onSendNow}
                onClick={() => { void withMessageLock(message.id, async () => { await onSendNow?.(message.id); }); }}
              ><SendHorizontal aria-hidden="true" className="size-3" />Enviar agora</Button></Hint>
              <Hint content="Retirar da fila e editar"><Button
                type="button"
                variant="ghost"
                size="icon"
                className="size-7 shrink-0 cursor-pointer text-muted-foreground hover:text-foreground"
                aria-label={`Editar mensagem ${index + 1}`}
                disabled={locked || itemBusy || !onEdit}
                onClick={() => { void withMessageLock(message.id, async () => { await onEdit?.(message.id); }); }}
              ><Pencil aria-hidden="true" className="size-3.5" /></Button></Hint>
              <Hint content="Cancelar e excluir"><Button
                type="button"
                variant="ghost"
                size="icon"
                className="size-7 shrink-0 cursor-pointer text-muted-foreground hover:text-destructive"
                aria-label={`Excluir mensagem ${index + 1}`}
                disabled={locked || itemBusy || !onDelete}
                onClick={event => {
                  event.stopPropagation();
                  setDeleteTarget(message);
                }}
              ><Trash2 aria-hidden="true" className="size-3.5" /></Button></Hint>
            </div>;
          }}</SortableItem>)}
        </SortableList>
      </div>
    </section>
    <AlertDialog open={deleteTarget !== null} onOpenChange={open => {
      if (!open && !(deleteTarget && busyIds.has(deleteTarget.id))) setDeleteTarget(null);
    }}>
      <AlertDialogContent className="dark">
        <AlertDialogHeader>
          <AlertDialogTitle>Excluir mensagem agendada?</AlertDialogTitle>
          <AlertDialogDescription>A mensagem será removida da fila e não voltará para o campo de texto.</AlertDialogDescription>
        </AlertDialogHeader>
        {deleteTarget && <p className="line-clamp-3 rounded-md border border-border bg-muted/35 px-3 py-2 text-xs text-foreground/80">{deleteTarget.content}</p>}
        <AlertDialogFooter>
          <AlertDialogCancel className="cursor-pointer" disabled={deleteTarget ? busyIds.has(deleteTarget.id) : false}>Manter na fila</AlertDialogCancel>
          <AlertDialogAction data-confirm-action variant="destructive" className="cursor-pointer" disabled={!deleteTarget || busyIds.has(deleteTarget.id)} onClick={() => { void confirmDelete(); }}>Excluir mensagem</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </>;
}
