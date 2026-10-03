import { useRef, useState } from "react";
import { ListOrdered, Pencil, SendHorizontal, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import type { QueuedMessage } from "@/core/chat";
import type { RemoteAction } from "./PendingForms";

export function RemoteQueuedMessages({ conversationId, messages, running, compacting, busy, onAction }: {
  conversationId: string; messages: QueuedMessage[]; running: boolean; compacting: boolean; busy: boolean; onAction: RemoteAction;
}) {
  if (!messages.length) return null;
  return <section aria-label="Mensagens na fila" className="remote-queue flex min-w-0 flex-col gap-2">
    <div className="flex items-center gap-2"><ListOrdered aria-hidden="true" className="size-4 text-onedark-cyan" /><p className="micro-label">Na fila</p><Badge variant="secondary">{messages.length}</Badge></div>
    {messages.map((message, index) => <QueuedMessageCard key={`${conversationId}:${message.id}`} conversationId={conversationId} message={message} index={index} running={running} disabled={busy || compacting} onAction={onAction} />)}
  </section>;
}

function QueuedMessageCard({ conversationId, message, index, running, disabled, onAction }: {
  conversationId: string; message: QueuedMessage; index: number; running: boolean; disabled: boolean; onAction: RemoteAction;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(message.content);
  const [pending, setPending] = useState(false);
  const lock = useRef(false);
  const locked = disabled || pending;
  const act = async (method: "queue_edit" | "queue_delete" | "queue_send_now") => {
    if (lock.current || disabled) return;
    lock.current = true; setPending(true);
    try {
      const accepted = await onAction(method, { conversationId, messageId: message.id, ...(method === "queue_edit" ? { content: draft.trim() } : {}) });
      if (accepted) setEditing(false);
    } finally { lock.current = false; setPending(false); }
  };
  return <Card size="sm" className="remote-queue-card" aria-label={`Mensagem na fila ${index + 1}`}>
    <CardHeader><CardTitle className="text-xs text-muted-foreground">Mensagem {index + 1}</CardTitle></CardHeader>
    <CardContent className="flex flex-col gap-2">{editing ? <FieldGroup><Field><FieldLabel htmlFor={`queue-${message.id}`} className="sr-only">Editar mensagem {index + 1}</FieldLabel><Textarea id={`queue-${message.id}`} autoFocus maxLength={64000} className="min-h-24 resize-y" value={draft} disabled={locked} onChange={event => setDraft(event.target.value)} /></Field></FieldGroup> : <p className="whitespace-pre-wrap break-words text-sm">{message.content}</p>}
      {message.parts?.some(part => part.type !== "text") && <div className="flex flex-wrap gap-1">{message.parts.filter(part => part.type !== "text").map(part => <Badge key={part.type === "skill" ? part.id : part.attachment.id} variant="outline" className="max-w-full whitespace-normal break-all">{part.type === "skill" ? `/${part.name}` : part.attachment.name}</Badge>)}</div>}
    </CardContent>
    <CardFooter className="flex-wrap gap-1">
      {editing ? <>
        <Button size="sm" className="cursor-pointer" disabled={locked || !draft.trim()} onClick={() => { void act("queue_edit"); }}>Salvar mensagem</Button>
        <Button size="sm" variant="ghost" className="cursor-pointer" disabled={locked} onClick={() => setEditing(false)}>Voltar</Button>
      </> : <>
        <Button size="sm" variant="outline" className="cursor-pointer" disabled={locked || !running} aria-label={`Enviar mensagem ${index + 1} agora`} onClick={() => { void act("queue_send_now"); }}><SendHorizontal data-icon="inline-start" />Enviar agora</Button>
        <Button size="sm" variant="ghost" className="cursor-pointer" disabled={locked} aria-label={`Editar mensagem ${index + 1}`} onClick={() => { setDraft(message.content); setEditing(true); }}><Pencil data-icon="inline-start" />Editar</Button>
        <Button size="sm" variant="ghost" className="cursor-pointer" disabled={locked} aria-label={`Cancelar envio da mensagem ${index + 1}`} onClick={() => { void act("queue_delete"); }}><X data-icon="inline-start" />Cancelar</Button>
      </>}
    </CardFooter>
  </Card>;
}
