import { useState } from "react";
import type { Project, Workspace } from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

export function MoveProjectDialog({ project, workspaces, library, onClose }: { project: Project; workspaces: Workspace[]; library: LibraryController; onClose: () => void }) {
  const choices = workspaces.filter(item => item.id !== project.workspaceId);
  const [target, setTarget] = useState(choices[0]?.id ?? "");
  return <Dialog open onOpenChange={open => { if (!open && !library.pending) onClose(); }}><DialogContent className="dark sm:max-w-md">
    <DialogHeader><DialogTitle>Mover {project.name}</DialogTitle><DialogDescription>As conversas acompanham o projeto. A pasta permanece no mesmo local.</DialogDescription></DialogHeader>
    <form className="space-y-4" onSubmit={event => { event.preventDefault(); if (target && !library.pending) void library.moveProject(project.id, target).then(saved => { if (saved) onClose(); }); }}>
      {choices.length ? <Select items={choices.map(w => ({ value: w.id, label: w.name }))} value={target} onValueChange={value => setTarget(value ?? "")} disabled={library.pending}><SelectTrigger aria-label="Workspace de destino" className="w-full cursor-pointer"><SelectValue /></SelectTrigger><SelectContent>{choices.map(w => <SelectItem key={w.id} value={w.id}>{w.name}</SelectItem>)}</SelectContent></Select> : <p className="text-sm text-muted-foreground">Crie outro workspace para mover este projeto.</p>}
      {library.error && <p role="alert" className="text-sm text-destructive">{library.error}</p>}
      <DialogFooter className="m-0 bg-transparent p-0"><Button type="button" variant="outline" disabled={library.pending} onClick={onClose}>Cancelar</Button><Button type="submit" disabled={!target || library.pending}>{library.pending ? "Movendo…" : "Mover projeto"}</Button></DialogFooter>
    </form>
  </DialogContent></Dialog>;
}
