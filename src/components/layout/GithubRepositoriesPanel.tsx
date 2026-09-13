import { ArrowDown, ArrowUp, CircleAlert, FilePenLine, FolderGit2, GitBranch, RefreshCw } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Hint } from "@/components/ui/hint";
import { useProjectRepositories } from "@/hooks/use-project-repositories";

export function GithubRepositoriesPanel({ projectId }: { projectId: string }) {
  const state = useProjectRepositories(projectId);
  return <section aria-label="Estado dos repositórios GitHub" className="flex h-full min-h-0 flex-col">
    <header className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3"><FolderGit2 className="size-3.5 text-onedark-cyan" /><h2 className="text-xs font-medium">Repositórios</h2><Badge variant="secondary" className="font-mono text-[9px]">{state.repositories.length}</Badge><Hint content="Atualizar estado local do Git"><Button type="button" variant="ghost" size="icon-sm" aria-label="Atualizar repositórios" className="ml-auto cursor-pointer text-muted-foreground" disabled={state.loading} onClick={() => { void state.refresh(); }}><RefreshCw className={`size-3.5 ${state.loading ? "animate-spin" : ""}`} /></Button></Hint></header>
    <ScrollArea className="min-h-0 flex-1"><div className="space-y-2 p-2.5">
      {state.loading && state.repositories.length === 0 && <div role="status" aria-label="Consultando repositórios" className="space-y-2"><Skeleton className="h-32" /><Skeleton className="h-32" /></div>}
      {state.error && <div role="alert" className="rounded-md border border-destructive/25 bg-destructive/5 p-3 text-xs text-destructive">{state.error}</div>}
      {!state.loading && !state.error && state.repositories.length === 0 && <div className="rounded-md border border-dashed border-border px-4 py-8 text-center"><FolderGit2 className="mx-auto mb-2 size-6 text-muted-foreground" /><p className="text-xs font-medium">Nenhum repositório configurado</p><p className="mt-1 text-[11px] leading-4 text-muted-foreground">Adicione as raízes Git em Detalhes → Opções.</p></div>}
      {state.repositories.map(repository => {
        const changes = repository.staged + repository.unstaged + repository.untracked;
        return <Card key={repository.id} role="article" aria-label={`GitHub ${repository.name}`} size="sm" className="gap-0 overflow-hidden rounded-md py-0">
          <div className="p-3"><div className="flex min-w-0 items-start gap-2.5"><span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-secondary text-onedark-cyan"><FolderGit2 className="size-3.5" /></span><div className="min-w-0 flex-1"><h3 className="truncate text-xs font-medium">{repository.name}</h3><p className="mt-0.5 truncate font-mono text-[9px] text-muted-foreground">{repository.path}</p></div>{repository.available ? <span className="mt-1 size-1.5 shrink-0 rounded-full bg-onedark-green shadow-[0_0_7px_var(--color-onedark-green)]" /> : <CircleAlert className="size-3.5 shrink-0 text-onedark-yellow" />}</div>
          {repository.description && <p className="mt-2 line-clamp-2 text-[11px] leading-4 text-muted-foreground">{repository.description}</p>}
          {repository.available ? <>
            <div className="mt-3 flex items-center gap-1.5"><Badge variant="outline" className="min-w-0 gap-1 font-mono text-[9px]"><GitBranch className="size-3 shrink-0" /><span className="truncate">{repository.branch ?? "sem branch"}</span></Badge><Badge variant="outline" className={`gap-0.5 font-mono text-[9px] ${repository.ahead ? "text-onedark-green" : "text-muted-foreground"}`}><ArrowUp className="size-3" />{repository.ahead}</Badge><Badge variant="outline" className={`gap-0.5 font-mono text-[9px] ${repository.behind ? "text-onedark-yellow" : "text-muted-foreground"}`}><ArrowDown className="size-3" />{repository.behind}</Badge></div>
            <dl className="mt-3 grid grid-cols-3 divide-x divide-border border-y border-border py-2 text-center"><div><dt className="micro-label text-[8px] text-muted-foreground">Staged</dt><dd className="mt-0.5 font-mono text-xs tabular-nums text-onedark-green">{repository.staged}</dd></div><div><dt className="micro-label text-[8px] text-muted-foreground">Alterados</dt><dd className="mt-0.5 font-mono text-xs tabular-nums text-primary">{repository.unstaged}</dd></div><div><dt className="micro-label text-[8px] text-muted-foreground">Novos</dt><dd className="mt-0.5 font-mono text-xs tabular-nums text-onedark-yellow">{repository.untracked}</dd></div></dl>
            <div className="mt-2 flex items-center gap-1.5 text-[9px] text-muted-foreground"><FilePenLine className="size-3" /><span>{changes ? `${changes} ${changes === 1 ? "alteração local" : "alterações locais"}` : "Árvore de trabalho limpa"}</span><span className="ml-auto min-w-0 truncate font-mono">{repository.upstream ?? "sem upstream"}</span></div>
            {repository.remoteUrl && <Hint content={repository.remoteUrl}><p className="mt-1.5 cursor-default truncate font-mono text-[9px] text-muted-foreground">{repository.remoteUrl}</p></Hint>}
          </> : <p role="status" className="mt-3 text-[11px] leading-4 text-onedark-yellow">{repository.error}</p>}
          </div>
        </Card>;
      })}
    </div></ScrollArea>
  </section>;
}
