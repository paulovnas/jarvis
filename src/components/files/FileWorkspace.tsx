import { Component, lazy, Suspense, type ReactNode } from "react";
import { FileSearch, LockKeyhole, MessageSquare, RefreshCw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Skeleton } from "@/components/ui/skeleton";
import { fileName } from "@/core/project-files";
import type { ProjectFilesController } from "@/hooks/use-project-files";
import { FileIcon } from "./FileIcon";

const CodeViewer = lazy(async () => {
  // Monaco resolves translated labels while its modules initialize. Finish the
  // locale import before loading the editor's dependency graph.
  await import("./monaco-locale");
  return import("./CodeViewer");
});

function FileSkeleton() {
  return <div role="status" aria-label="Carregando visualização do arquivo" className="h-full space-y-3 p-5"><Skeleton className="h-4 w-3/4" /><Skeleton className="h-4 w-1/2" /><Skeleton className="h-4 w-4/5" /><Skeleton className="h-4 w-2/3" /></div>;
}

class ViewerBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() { return this.state.failed ? <p role="alert" className="p-5 text-sm text-destructive">Não foi possível carregar o visualizador. Reabra o Jarvis para tentar novamente.</p> : this.props.children; }
}

export function FileWorkspace({ files, terminalLauncher, children }: { files?: ProjectFilesController; terminalLauncher?: ReactNode; children: ReactNode }) {
  if (!files?.projectId) return children;
  const active = files.tabs.activePath;
  const select = (value: unknown) => files.select(typeof value === "string" && value.startsWith("file:") ? value.slice(5) : null);
  const close = (event: React.MouseEvent<HTMLButtonElement>) => { const path = event.currentTarget.dataset.path; if (path) files.close(path); };
  return <Tabs value={active ? `file:${active}` : "chat"} onValueChange={select} className="h-full min-h-0 min-w-0 flex-1 gap-0">
    <div className="flex min-h-9 min-w-0 shrink-0 items-center gap-2 border-b border-border bg-sidebar px-2">
      <div className="min-w-0 flex-1 overflow-x-auto overflow-y-hidden"><TabsList aria-label="Chat e arquivos abertos" className="h-9 justify-start gap-1 rounded-none bg-transparent p-0">
        <TabsTrigger value="chat" className="h-7 flex-none cursor-pointer gap-1.5 px-3 text-xs"><MessageSquare aria-hidden="true" className="size-3.5" />Chat</TabsTrigger>
        {files.tabs.paths.map(path => <div className="group/file-tab relative shrink-0" key={path}>
          <TabsTrigger value={`file:${path}`} title={path} className="h-7 max-w-64 cursor-pointer gap-1.5 pl-2 pr-7 font-mono text-[11px]"><FileIcon path={path} /><span className="truncate">{fileName(path)}{files.tabs.paths.some(other => other !== path && fileName(other) === fileName(path)) && <span className="ml-1 text-muted-foreground">· {path.slice(0, path.lastIndexOf("/")) || "/"}</span>}</span></TabsTrigger>
          <Button type="button" variant="ghost" size="icon" title={`Fechar ${path}`} aria-label={`Fechar arquivo ${path}`} data-path={path} onClick={close} className="absolute inset-y-0 right-0.5 my-auto size-5 cursor-pointer opacity-60 group-hover/file-tab:opacity-100 focus-visible:opacity-100 active:not-aria-[haspopup]:translate-y-0"><X className="size-3" /></Button>
        </div>)}
      </TabsList></div>
      {terminalLauncher}
    </div>
    <TabsContent value="chat" keepMounted inert={!!active} className={`min-h-0 min-w-0 flex-1 flex-col ${active ? "hidden" : "flex"}`}>{children}</TabsContent>
    {active && <TabsContent value={`file:${active}`} className="m-0 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3 text-xs text-muted-foreground"><FileIcon path={active} /><span className="min-w-0 flex-1 truncate font-mono" title={active}>{active}</span><LockKeyhole aria-hidden="true" className="size-3" /><span className="shrink-0 text-[10px]">Somente leitura</span><Button type="button" variant="ghost" size="icon" aria-label="Atualizar arquivo" title="Atualizar arquivo" onClick={files.refresh} disabled={files.active.loading} className="size-6 cursor-pointer"><RefreshCw className="size-3.5" /></Button></div>
      <div className="min-h-0 min-w-0 flex-1">
        {files.active.loading ? <FileSkeleton /> : files.active.error ? <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center"><FileSearch className="size-8 text-muted-foreground" /><p role="alert" className="max-w-md text-sm text-muted-foreground">{files.active.error}</p><Button type="button" variant="outline" onClick={files.refresh} className="cursor-pointer">Tentar novamente</Button></div> : files.active.data ? <ViewerBoundary key={files.projectId}><Suspense fallback={<FileSkeleton />}><CodeViewer file={files.active.data} paths={files.tabs.paths} visible /></Suspense></ViewerBoundary> : null}
      </div>
      {files.active.data && <div className="flex h-6 shrink-0 items-center justify-end gap-3 border-t border-border px-3 font-mono text-[10px] text-muted-foreground"><span>{files.active.data.encoding}</span><span>{files.active.data.size.toLocaleString("pt-BR")} bytes</span></div>}
    </TabsContent>}
  </Tabs>;
}
