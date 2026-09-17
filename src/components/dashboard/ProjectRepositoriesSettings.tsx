import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowDown, ArrowUp, FilePenLine, FolderGit2, FolderOpen, GitBranch, Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Input, Textarea } from "@/components/TextInput";
import { libraryError } from "@/core/library";
import { deleteProjectRepository, saveProjectRepository, type ProjectRepository, type ProjectRepositoryInput } from "@/core/project-repositories";
import { useProjectRepositories } from "@/hooks/use-project-repositories";
import { Hint } from "@/components/ui/hint";

function folderName(path: string) {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || "Repositório";
}

export function ProjectRepositoriesSettings({ projectId, projectPath }: { projectId: string; projectPath: string }) {
  const repositories = useProjectRepositories(projectId);
  const [editor, setEditor] = useState<ProjectRepositoryInput | null>(null);
  const [saving, setSaving] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [removing, setRemoving] = useState<ProjectRepository | null>(null);
  const [deleting, setDeleting] = useState(false);

  const chooseDirectory = async (current = projectPath) => {
    if (selecting) return null;
    setSelecting(true);
    try {
      const selected = await open({ directory: true, multiple: false, defaultPath: current, title: "Selecionar raiz do repositório Git" });
      return typeof selected === "string" ? selected : null;
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível abrir o seletor de pastas."));
      return null;
    } finally { setSelecting(false); }
  };

  const add = async () => {
    const directory = await chooseDirectory();
    if (directory) setEditor({ directory, name: folderName(directory), description: "" });
  };

  const save = async () => {
    if (!editor || saving || !editor.name.trim()) return;
    setSaving(true);
    try {
      await saveProjectRepository(projectId, editor);
      setEditor(null);
      await repositories.refresh();
      toast.success(editor.id ? "Repositório atualizado" : "Repositório adicionado");
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível salvar o repositório."));
    } finally { setSaving(false); }
  };

  const remove = async () => {
    if (!removing || deleting) return;
    setDeleting(true);
    try {
      await deleteProjectRepository(projectId, removing.id);
      setRemoving(null);
      await repositories.refresh();
      toast.success("Repositório removido da configuração");
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível remover o repositório."));
    } finally { setDeleting(false); }
  };

  return <>
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-4 border-b border-border">
        <div className="min-w-0"><div className="flex items-center gap-2"><FolderGit2 className="size-4 text-onedark-cyan" /><CardTitle>Repositórios Git</CardTitle><Badge variant="secondary" className="font-mono text-[10px]">{repositories.repositories.length}</Badge></div><CardDescription className="mt-1 max-w-3xl leading-5">Cadastre cada raiz Git que compõe o projeto. Nome, finalidade e caminho entram no contexto dos agentes antes do trabalho começar.</CardDescription></div>
        <Button type="button" variant="outline" size="sm" className="shrink-0 cursor-pointer gap-2" disabled={selecting} onClick={() => { void add(); }}><Plus className="size-3.5" />{selecting ? "Selecionando…" : "Adicionar"}</Button>
      </CardHeader>
      <CardContent className="space-y-3 pt-5">
        {repositories.loading && repositories.repositories.length === 0 && <div role="status" aria-label="Carregando repositórios" className="grid gap-3 md:grid-cols-2"><Skeleton className="h-32" /><Skeleton className="h-32" /></div>}
        {repositories.error && <Alert variant="destructive"><FolderGit2 /><AlertTitle>Repositórios indisponíveis</AlertTitle><AlertDescription>{repositories.error}</AlertDescription></Alert>}
        {!repositories.loading && !repositories.error && repositories.repositories.length === 0 && <div className="rounded-lg border border-dashed border-border p-6 text-center"><FolderGit2 className="mx-auto mb-2 size-6 text-muted-foreground" /><p className="text-sm font-medium">Nenhum repositório configurado</p><p className="mx-auto mt-1 max-w-xl text-xs leading-5 text-muted-foreground">Comece pela raiz do projeto ou selecione uma pasta como Frontend, Backend ou Microserviço.</p><Button type="button" variant="outline" size="sm" className="mt-4 cursor-pointer gap-2" onClick={() => { void add(); }}><FolderOpen className="size-3.5" />Selecionar repositório</Button></div>}
        {repositories.repositories.length > 0 && <div className="grid gap-3 md:grid-cols-2">
          {repositories.repositories.map(repository => <article key={repository.id} aria-label={`Repositório ${repository.name}`} className="rounded-lg border border-border bg-sidebar/45 p-4 shadow-[inset_0_1px_0_rgb(255_255_255/0.035)]">
            <div className="flex items-start gap-3"><span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-secondary text-onedark-cyan"><FolderGit2 className="size-4" /></span><div className="min-w-0 flex-1"><Hint content={repository.name} whenTruncated><h3 className="truncate text-sm font-medium">{repository.name}</h3></Hint><Hint content={repository.path} whenTruncated><p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">{repository.path}</p></Hint></div><Hint content={`Editar ${repository.name}`}><Button type="button" variant="ghost" size="icon-sm" className="cursor-pointer text-muted-foreground" aria-label={`Editar ${repository.name}`} onClick={() => setEditor({ id: repository.id, directory: repository.directory, name: repository.name, description: repository.description })}><Pencil className="size-3.5" /></Button></Hint><Hint content={`Remover ${repository.name}`}><Button type="button" variant="ghost" size="icon-sm" className="cursor-pointer text-destructive" aria-label={`Remover ${repository.name}`} onClick={() => setRemoving(repository)}><Trash2 className="size-3.5" /></Button></Hint></div>
            {repository.description && <p className="mt-3 line-clamp-2 text-xs leading-5 text-muted-foreground">{repository.description}</p>}
            {repository.available ? <div className="mt-3 flex flex-wrap gap-1.5 border-t border-border pt-3"><Badge variant="outline" className="gap-1 font-mono text-[9px]"><GitBranch className="size-3" />{repository.branch ?? "sem branch"}</Badge>{repository.ahead > 0 && <Badge variant="outline" className="gap-1 font-mono text-[9px] text-onedark-green"><ArrowUp className="size-3" />{repository.ahead}</Badge>}{repository.behind > 0 && <Badge variant="outline" className="gap-1 font-mono text-[9px] text-onedark-yellow"><ArrowDown className="size-3" />{repository.behind}</Badge>}{repository.staged + repository.unstaged + repository.untracked > 0 && <Badge variant="outline" className="gap-1 font-mono text-[9px] text-primary"><FilePenLine className="size-3" />{repository.staged + repository.unstaged + repository.untracked}</Badge>}</div> : <p role="status" className="mt-3 border-t border-border pt-3 text-[11px] text-onedark-yellow">{repository.error}</p>}
          </article>)}
        </div>}
      </CardContent>
    </Card>

    <Dialog open={Boolean(editor)} onOpenChange={open => { if (!open && !saving) setEditor(null); }}>
      <DialogContent showCloseButton={false} className="dark sm:max-w-lg">
        <DialogHeader><DialogTitle>{editor?.id ? "Editar repositório" : "Adicionar repositório"}</DialogTitle><DialogDescription>A pasta deve ser a raiz exata de um repositório Git dentro deste projeto.</DialogDescription></DialogHeader>
        {editor && <div className="space-y-4">
          <div className="space-y-2"><Label htmlFor="repository-directory">Pasta</Label><div className="flex gap-2"><Input id="repository-directory" aria-label="Pasta do repositório" value={editor.directory} readOnly className="min-w-0 flex-1 font-mono text-xs" /><Button type="button" variant="outline" className="cursor-pointer" disabled={selecting || saving} onClick={() => { void chooseDirectory(editor.directory).then(directory => { if (directory) setEditor(current => current ? { ...current, directory } : current); }); }}><FolderOpen className="size-4" />Alterar</Button></div></div>
          <div className="space-y-2"><Label htmlFor="repository-name">Nome</Label><Input id="repository-name" aria-label="Nome do repositório" value={editor.name} maxLength={80} disabled={saving} onChange={event => setEditor(current => current ? { ...current, name: event.target.value } : current)} placeholder="Ex.: Backend" /></div>
          <div className="space-y-2"><Label htmlFor="repository-description">Descrição</Label><Textarea id="repository-description" aria-label="Descrição do repositório" value={editor.description} maxLength={500} disabled={saving} onChange={event => setEditor(current => current ? { ...current, description: event.target.value } : current)} placeholder="Explique aos agentes a responsabilidade deste repositório." className="min-h-24 resize-y text-xs leading-5" /><p className="text-right font-mono text-[10px] text-muted-foreground">{editor.description.length}/500</p></div>
        </div>}
        <DialogFooter><Button type="button" variant="outline" className="cursor-pointer" disabled={saving} onClick={() => setEditor(null)}>Cancelar</Button><Button type="button" className="cursor-pointer" disabled={saving || !editor?.name.trim() || !editor.directory} onClick={() => { void save(); }}>{saving ? "Salvando…" : "Salvar repositório"}</Button></DialogFooter>
      </DialogContent>
    </Dialog>

    <AlertDialog open={Boolean(removing)} onOpenChange={open => { if (!open && !deleting) setRemoving(null); }}>
      <AlertDialogContent className="dark"><AlertDialogHeader><AlertDialogTitle>Remover {removing?.name}?</AlertDialogTitle><AlertDialogDescription>A pasta e o repositório Git serão preservados. O Jarvis deixará de incluir esta configuração no contexto dos agentes.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel disabled={deleting} className="cursor-pointer">Cancelar</AlertDialogCancel><AlertDialogAction variant="destructive" disabled={deleting} className="cursor-pointer" onClick={event => { event.preventDefault(); void remove(); }}>{deleting ? "Removendo…" : "Remover configuração"}</AlertDialogAction></AlertDialogFooter></AlertDialogContent>
    </AlertDialog>
  </>;
}
