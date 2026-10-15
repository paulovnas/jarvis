import { useState } from "react";
import type { Conversation } from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

export function MoveGlobalChatDialog({ conversation, library, onClose }: { conversation: Conversation; library: LibraryController; onClose: () => void }) {
  const projects = library.snapshot?.projects ?? [];
  const [target, setTarget] = useState(projects[0]?.id ?? "");
  const label = (id: string, name: string) => `${library.snapshot?.workspaces.find(space => space.id === id)?.name ?? ""} / ${name}`;
  return <Dialog open onOpenChange={open => { if (!open && !library.pending) onClose(); }}><DialogContent className="dark sm:max-w-md">
    <DialogHeader><DialogTitle>Mover conversa para um projeto</DialogTitle><DialogDescription>O histórico de “{conversation.title}” será preservado. As próximas mensagens usarão a pasta do projeto.</DialogDescription></DialogHeader>
    <form className="space-y-4" onSubmit={event => { event.preventDefault(); if (target && !library.pending) void library.moveGlobalConversation(conversation.id, target).then(saved => { if (saved) onClose(); }); }}>
      {projects.length ? <Select items={projects.map(project => ({ value: project.id, label: label(project.workspaceId, project.name) }))} value={target} onValueChange={value => setTarget(value ?? "")} disabled={library.pending}><SelectTrigger aria-label="Projeto de destino" className="w-full cursor-pointer"><SelectValue /></SelectTrigger><SelectContent>{projects.map(project => <SelectItem key={project.id} value={project.id} className="cursor-pointer">{label(project.workspaceId, project.name)}</SelectItem>)}</SelectContent></Select> : <p className="text-sm text-muted-foreground">Adicione um projeto para mover esta conversa.</p>}
      {library.error && <p role="alert" className="text-sm text-destructive">{library.error}</p>}
      <DialogFooter className="m-0 bg-transparent p-0"><Button type="button" variant="outline" className="cursor-pointer" disabled={library.pending} onClick={onClose}>Cancelar</Button><Button type="submit" className="cursor-pointer" disabled={!target || library.pending}>{library.pending ? "Movendo…" : "Mover conversa"}</Button></DialogFooter>
    </form>
  </DialogContent></Dialog>;
}
