import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Folder, Pencil, Plus, Terminal as TerminalIcon, X } from "lucide-react";
import { toast } from "sonner";
import { ConfirmationDialogContent } from "@/components/ConfirmationDialogContent";
import { Input } from "@/components/TextInput";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { libraryError } from "@/core/library";
import { rememberTerminalPanel } from "@/core/desktop-layout";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";
import {
  TERMINAL_STATUS_LABELS,
  terminalSchema,
  type ProjectTerminal,
} from "@/core/terminals";

const TerminalSurface = lazy(() => import("./TerminalSurface").then(module => ({ default: module.TerminalSurface })));

function RenameTerminalPopover({
  open,
  terminal,
  pending,
  onOpenChange,
  onRename,
}: {
  open: boolean;
  terminal: ProjectTerminal;
  pending: boolean;
  onOpenChange: (open: boolean) => void;
  onRename: (title: string) => Promise<void>;
}) {
  const [title, setTitle] = useState(terminal.title);

  return <Popover open={open} onOpenChange={onOpenChange}>
    <PopoverTrigger render={<Button type="button" tabIndex={-1} aria-hidden className="pointer-events-none absolute inset-0 size-full opacity-0" />} />
    <PopoverContent className="w-72 border border-border bg-card" align="start">
      <form className="flex gap-2" onSubmit={event => {
        event.preventDefault();
        void onRename(title);
      }}>
        <Input aria-label={`Novo nome para ${terminal.title}`} autoFocus value={title} maxLength={80} disabled={pending} onChange={event => setTitle(event.target.value)} />
        <Button type="submit" size="sm" className="cursor-pointer" disabled={pending || !title.trim()}>Salvar</Button>
      </form>
    </PopoverContent>
  </Popover>;
}

export function TerminalWorkspace({ projectId }: { projectId: string }) {
  return <ProjectTerminalWorkspace key={projectId} projectId={projectId} />;
}

function ProjectTerminalWorkspace({ projectId }: { projectId: string }) {
  const mounted = useRef(false);
  const { layout, updateLayout } = useDesktopLayout();
  const layoutKey = `project:${projectId}`;
  const activeId = layout.terminalPanels[layoutKey]?.activeTerminalId;
  const setActiveId = (activeTerminalId: string | null) => updateLayout(current => rememberTerminalPanel(current, layoutKey, { activeTerminalId }));
  const [terminals, setTerminals] = useState<ProjectTerminal[]>([]);
  const [loadedProjectId, setLoadedProjectId] = useState<string>();
  const [creating, setCreating] = useState(false);
  const [closing, setClosing] = useState<ProjectTerminal | null>(null);
  const [closingPending, setClosingPending] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renamingPending, setRenamingPending] = useState(false);

  useEffect(() => {
    let current = true;
    let revision = 0;
    mounted.current = true;
    let unlisten: (() => void) | undefined;
    const refresh = async () => {
      const request = ++revision;
      try {
        const next = terminalSchema.array().parse(await invoke("list_project_terminals", { projectId }));
        if (!current || request !== revision) return;
        setTerminals(next);
      } catch (error) {
        if (current && request === revision) toast.error(libraryError(error, "Não foi possível carregar os terminais."));
      } finally {
        if (current && request === revision) setLoadedProjectId(projectId);
      }
    };
    void listen<{ projectId: string }>("terminals:changed", event => {
      if (current && event.payload.projectId === projectId) void refresh();
    }).then(stop => {
      if (!current) {
        stop();
        return;
      }
      unlisten = stop;
      void refresh();
    }).catch(error => {
      if (current) {
        toast.error(libraryError(error, "Não foi possível acompanhar os terminais."));
        void refresh();
      }
    });
    return () => {
      current = false;
      mounted.current = false;
      unlisten?.();
    };
  }, [projectId]);

  const create = async () => {
    setCreating(true);
    try {
      const terminal = terminalSchema.parse(await invoke("create_project_terminal", { projectId }));
      if (!mounted.current) return;
      setTerminals(items => [...items.filter(item => item.id !== terminal.id), terminal]);
      setActiveId(terminal.id);
      toast.success("Terminal aberto.");
    } catch (error) {
      if (mounted.current) toast.error(libraryError(error, "Não foi possível abrir o terminal."));
    } finally {
      if (mounted.current) setCreating(false);
    }
  };

  const rename = async (terminal: ProjectTerminal, title: string) => {
    setRenamingPending(true);
    try {
      await invoke("rename_project_terminal", { projectId, id: terminal.id, title });
      if (!mounted.current) return;
      setTerminals(items => items.map(item => item.id === terminal.id ? { ...item, title: title.trim() } : item));
      setRenamingId(null);
    } catch (error) {
      if (mounted.current) toast.error(libraryError(error, "Não foi possível renomear o terminal."));
    } finally {
      if (mounted.current) setRenamingPending(false);
    }
  };

  const close = async () => {
    if (!closing) return;
    setClosingPending(true);
    try {
      await invoke("close_project_terminal", { projectId, id: closing.id, confirmed: true });
      if (!mounted.current) return;
      setTerminals(items => items.filter(item => item.id !== closing.id));
      if (active?.id === closing.id) setActiveId(terminals.find(item => item.id !== closing.id && item.projectId === projectId)?.id ?? null);
      setClosing(null);
      toast.success("Terminal fechado.");
    } catch (error) {
      if (mounted.current) toast.error(libraryError(error, "Não foi possível fechar o terminal."));
    } finally {
      if (mounted.current) setClosingPending(false);
    }
  };

  const visibleTerminals = terminals.filter(terminal => terminal.projectId === projectId);
  const active = visibleTerminals.find(terminal => terminal.id === activeId) ?? visibleTerminals[0] ?? null;
  const loading = loadedProjectId !== projectId;

  return <>
    <section aria-label="Terminais do projeto" className="dark flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-sidebar text-foreground">
        <div className="min-h-0 min-w-0 flex-1">
            {loading ? <div role="status" aria-label="Carregando terminais do projeto" className="flex h-full flex-col gap-3"><Skeleton className="h-8 w-56" /><Skeleton className="min-h-0 flex-1" /></div> : visibleTerminals.length === 0 ? <Empty className="h-full rounded-lg border border-border">
              <EmptyHeader><EmptyMedia variant="icon"><TerminalIcon /></EmptyMedia><EmptyTitle>Nenhum terminal aberto</EmptyTitle><EmptyDescription>Abra um shell na raiz do projeto para trabalhar aqui.</EmptyDescription></EmptyHeader>
              <Button type="button" size="sm" className="cursor-pointer" disabled={creating} onClick={() => { void create(); }}><Plus data-icon="inline-start" />Novo Terminal</Button>
            </Empty> : <Tabs value={active?.id} onValueChange={setActiveId} className="h-full min-h-0 min-w-0 gap-0">
              <div role="group" aria-label="Abas dos terminais" className="min-w-0 shrink-0 overflow-x-auto overflow-y-hidden border-b border-border px-2 py-1">
                <div className="flex w-max min-w-full items-center gap-1">
                  <TabsList aria-label="Terminais abertos" variant="line" className="h-8 shrink-0 justify-start gap-1 p-0">
                    {visibleTerminals.map(terminal => <div key={terminal.id} className="group/tab relative shrink-0">
                      <ContextMenu>
                        <ContextMenuTrigger render={<TabsTrigger value={terminal.id} aria-label={terminal.title} className="h-8 max-w-52 cursor-pointer pr-7 font-mono text-[11px]" />}>
                          <span className="truncate">{terminal.title}</span>
                          {terminal.origin === "agent" && <span className="text-[9px] text-muted-foreground">IA</span>}
                        </ContextMenuTrigger>
                        <ContextMenuContent>
                          <ContextMenuItem className="cursor-pointer" onClick={() => setRenamingId(terminal.id)}><Pencil />Renomear</ContextMenuItem>
                        </ContextMenuContent>
                      </ContextMenu>
                      {/* Center without a transform: Button's pressed animation also uses translate. */}
                      <Hint content={`Fechar ${terminal.title}`}><Button type="button" variant="ghost" size="icon" aria-label={`Fechar ${terminal.title}`} aria-haspopup="dialog" className="absolute inset-y-0 right-0.5 z-10 my-auto size-5 cursor-pointer opacity-0 transition-opacity group-hover/tab:opacity-100 focus-visible:opacity-100" onPointerDown={event => event.stopPropagation()} onClick={event => { event.stopPropagation(); setClosing(terminal); }}><X className="size-3" /></Button></Hint>
                      <RenameTerminalPopover key={`${terminal.id}:${renamingId === terminal.id ? terminal.title : "closed"}`} open={renamingId === terminal.id} terminal={terminal} pending={renamingPending} onOpenChange={next => { if (!next) setRenamingId(null); }} onRename={title => rename(terminal, title)} />
                    </div>)}
                  </TabsList>
                  <Hint content="Novo terminal"><Button type="button" variant="outline" size="icon" aria-label="Novo terminal" className="size-7 shrink-0 cursor-pointer" disabled={creating} onClick={() => { void create(); }}><Plus className="size-3.5" /></Button></Hint>
                </div>
              </div>
              {active && <TabsContent value={active.id} className="flex min-h-0 min-w-0 flex-col overflow-hidden bg-sidebar" aria-label={active.title}>
                {active.status !== "running" && <div role="status" className="flex items-center justify-between gap-3 border-b border-border bg-card px-4 py-2 text-xs text-muted-foreground"><span>Processo encerrado{active.exitCode !== null ? ` · código ${active.exitCode}` : ""}</span><Button type="button" variant="outline" size="sm" disabled={creating} onClick={() => void create()} className="h-7 cursor-pointer text-xs"><Plus className="size-3.5" />Abrir novo terminal</Button></div>}
                <div className="min-h-0 flex-1"><Suspense fallback={<Skeleton role="status" aria-label="Carregando terminal" className="h-full w-full" />}><TerminalSurface projectId={projectId} terminal={active} /></Suspense></div>
              </TabsContent>}
            </Tabs>}
        </div>
        {active && <div className="flex shrink-0 items-center gap-2 border-t border-border bg-secondary/40 px-5 py-2 font-mono text-[11px] text-muted-foreground"><Folder className="size-3 shrink-0 text-onedark-cyan" /><Hint content={active.command ? `${active.cwd}\n${active.command}` : active.cwd}><span className="min-w-0 flex-1 truncate">{active.cwd}</span></Hint><span className={`shrink-0 ${active.status === "failed" ? "text-destructive" : active.status === "running" ? "text-onedark-green" : ""}`}>{TERMINAL_STATUS_LABELS[active.status]}</span></div>}
    </section>
    <AlertDialog open={closing !== null} onOpenChange={next => { if (!next && !closingPending) setClosing(null); }}>
      <ConfirmationDialogContent aria-describedby={undefined}>
        <AlertDialogHeader><AlertDialogTitle>Fechar {closing?.title}?</AlertDialogTitle><AlertDialogDescription>O shell e os processos iniciados por este terminal serão encerrados.</AlertDialogDescription></AlertDialogHeader>
        <AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={closingPending}>Cancelar</AlertDialogCancel><AlertDialogAction data-confirm-action className="cursor-pointer" disabled={closingPending} onClick={() => { void close(); }}>Fechar terminal</AlertDialogAction></AlertDialogFooter>
      </ConfirmationDialogContent>
    </AlertDialog>
  </>;
}
