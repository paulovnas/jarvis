import { lazy, Suspense, useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useGroupRef } from "react-resizable-panels";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Folder, PanelBottomClose, PanelBottomOpen, Pencil, Plus, Terminal as TerminalIcon, X } from "lucide-react";
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { libraryError } from "@/core/library";
import { DEFAULT_TERMINAL_PANEL, rememberTerminalPanel, type TerminalPanelLayout } from "@/core/desktop-layout";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";
import {
  TERMINAL_STATUS_LABELS,
  terminalSchema,
  type ChatTerminal,
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
  terminal: ChatTerminal;
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

export function TerminalWorkspace({ conversationId, children }: { conversationId?: string; children: (launcher: ReactNode) => ReactNode }) {
  const panelId = useId();
  const launcherRef = useRef<HTMLButtonElement>(null);
  const { layout, updateLayout } = useDesktopLayout();
  const { open, size: panelSize, activeTerminalId: activeId } = layout.terminalPanels[conversationId ?? ""] ?? DEFAULT_TERMINAL_PANEL;
  const groupRef = useGroupRef();
  const [animating, setAnimating] = useState(false);
  useLayoutEffect(() => { groupRef.current?.setLayout({ conversation: open ? 100 - panelSize : 100, terminals: open ? panelSize : 0 }); }, [groupRef, open, panelSize]);
  useEffect(() => {
    if (!animating) return;
    const timer = setTimeout(() => setAnimating(false), 240);
    return () => clearTimeout(timer);
  }, [animating, open]);
  const remember = (update: Partial<TerminalPanelLayout>) => {
    if (conversationId) updateLayout(current => rememberTerminalPanel(current, conversationId, update));
  };
  const setActiveId = (activeTerminalId: string | null) => remember({ activeTerminalId });
  const togglePanel = () => { setAnimating(true); remember({ open: !open }); };
  const [terminals, setTerminals] = useState<ChatTerminal[]>([]);
  const [loadedConversationId, setLoadedConversationId] = useState<string>();
  const [creating, setCreating] = useState(false);
  const [closing, setClosing] = useState<ChatTerminal | null>(null);
  const [closingPending, setClosingPending] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renamingPending, setRenamingPending] = useState(false);

  useEffect(() => {
    if (!conversationId) return;
    let current = true;
    let unlisten: (() => void) | undefined;
    const refresh = async () => {
      try {
        const next = terminalSchema.array().parse(await invoke("list_chat_terminals", { conversationId }));
        if (!current) return;
        setTerminals(next);
      } catch (error) {
        if (current) toast.error(libraryError(error, "Não foi possível carregar os terminais."));
      } finally {
        if (current) setLoadedConversationId(conversationId);
      }
    };
    void listen<{ conversationId: string }>("terminals:changed", event => {
      if (current && event.payload.conversationId === conversationId) void refresh();
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
      unlisten?.();
    };
  }, [conversationId]);

  const create = async () => {
    if (!conversationId) return;
    setCreating(true);
    try {
      const terminal = terminalSchema.parse(await invoke("create_chat_terminal", { conversationId }));
      setTerminals(items => [...items.filter(item => item.id !== terminal.id), terminal]);
      setActiveId(terminal.id);
      toast.success("Terminal aberto.");
    } catch (error) {
      toast.error(libraryError(error, "Não foi possível abrir o terminal."));
    } finally {
      setCreating(false);
    }
  };

  const rename = async (terminal: ChatTerminal, title: string) => {
    if (!conversationId) return;
    setRenamingPending(true);
    try {
      await invoke("rename_chat_terminal", { conversationId, id: terminal.id, title });
      setTerminals(items => items.map(item => item.id === terminal.id ? { ...item, title: title.trim() } : item));
      setRenamingId(null);
    } catch (error) {
      toast.error(libraryError(error, "Não foi possível renomear o terminal."));
    } finally {
      setRenamingPending(false);
    }
  };

  const close = async () => {
    if (!conversationId || !closing) return;
    setClosingPending(true);
    try {
      await invoke("close_chat_terminal", { conversationId, id: closing.id, confirmed: true });
      setTerminals(items => items.filter(item => item.id !== closing.id));
      if (activeId === closing.id) setActiveId(terminals.find(item => item.id !== closing.id && item.conversationId === conversationId)?.id ?? null);
      setClosing(null);
      toast.success("Terminal fechado.");
    } catch (error) {
      toast.error(libraryError(error, "Não foi possível fechar o terminal."));
    } finally {
      setClosingPending(false);
    }
  };

  const visibleTerminals = terminals.filter(terminal => terminal.conversationId === conversationId);
  const active = visibleTerminals.find(terminal => terminal.id === activeId) ?? visibleTerminals[0] ?? null;
  const loading = Boolean(conversationId && loadedConversationId !== conversationId);
  const terminalCount = visibleTerminals.length;
  const count = terminalCount;
  const triggerLabel = count === 0 ? "Abrir terminais" : count === 1 ? "1 terminal aberto" : `${count} terminais abertos`;

  const launcher = <Button
        ref={launcherRef}
        type="button"
        variant="ghost"
        size="icon"
        disabled={!conversationId}
        title={conversationId ? open ? "Recolher painel de terminais" : triggerLabel : "Abra uma conversa para usar o terminal"}
        aria-label={triggerLabel}
        aria-expanded={open}
        aria-controls={panelId}
        onClick={togglePanel}
        className={`relative size-7.5 shrink-0 cursor-pointer rounded-full transition-colors hover:bg-accent hover:text-foreground ${open ? "bg-primary/15 text-primary" : "bg-secondary text-foreground"}`}
      >
        {open ? <PanelBottomClose className="size-3.5 stroke-[2.2]" /> : <PanelBottomOpen className="size-3.5 stroke-[2.2]" />}
        {count > 0 && <Badge className="absolute -right-1.5 -top-1.5 min-w-4 justify-center border-border bg-primary px-1 py-0 text-[9px] text-primary-foreground">{count}</Badge>}
      </Button>;

  return <>
    <ResizablePanelGroup groupRef={groupRef} orientation="vertical" className={`min-h-0 min-w-0 flex-1 ${animating ? "panels-animating" : ""}`} onLayoutChanged={(panels, meta) => {
      if (!meta.isUserInteraction) return;
      if (panels.terminals === 0) remember({ open: false });
      else if (panels.terminals !== panelSize) remember({ size: panels.terminals });
    }}>
      <ResizablePanel id="conversation" defaultSize={`${100 - panelSize}%`} minSize="35%" className="flex min-h-0 min-w-0 flex-col">
        {children(launcher)}
      </ResizablePanel>
      <ResizableHandle aria-label="Redimensionar painel de terminais" disabled={!open} aria-hidden={!open} inert={!open} className={`cursor-row-resize bg-border hover:bg-primary/50 ${!open ? "invisible h-0 pointer-events-none" : ""}`} />
      <ResizablePanel id="terminals" collapsible defaultSize={open ? `${panelSize}%` : "0%"} minSize="20%" maxSize="65%" className="min-h-0 min-w-0">
      <section id={panelId} aria-label="Painel de terminais" inert={!open} aria-hidden={!open} className={`dark flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-sidebar text-foreground transition-transform duration-200 motion-reduce:transition-none ${open ? "" : "translate-y-full"}`}>
        <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border bg-card px-3 py-1">
          <span className="flex items-center gap-2 text-xs font-medium"><TerminalIcon className="size-3.5 text-onedark-green" />Terminais{count > 0 && <Badge variant="secondary" className="px-1 py-0 font-mono text-[9px]">{count}</Badge>}</span>
          <Button type="button" variant="ghost" size="icon" aria-label="Recolher painel de terminais" title="Recolher painel de terminais" className="size-6 shrink-0 cursor-pointer text-muted-foreground" onClick={() => { togglePanel(); launcherRef.current?.focus(); }}><PanelBottomClose className="size-3.5" /></Button>
        </div>
        <div className="min-h-0 min-w-0 flex-1">
            {loading ? <div className="flex h-full flex-col gap-3"><Skeleton className="h-8 w-56" /><Skeleton className="min-h-0 flex-1" /></div> : visibleTerminals.length === 0 ? <div className="flex h-full flex-col items-center justify-center gap-4 rounded-lg border border-dashed border-border bg-sidebar/50 text-center">
              <span className="flex size-11 items-center justify-center rounded-md border border-border bg-card text-primary"><TerminalIcon className="size-5" /></span>
              <div><p className="text-sm font-medium">Nenhum terminal aberto</p><p className="mt-1 text-xs text-muted-foreground">Abra um shell na raiz do projeto para trabalhar aqui.</p></div>
              <Button type="button" size="sm" className="cursor-pointer" disabled={creating} onClick={() => { void create(); }}><Plus className="size-4" />Novo Terminal</Button>
            </div> : <Tabs value={active?.id} onValueChange={setActiveId} className="h-full min-h-0 min-w-0 gap-0">
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
                      <Button type="button" variant="ghost" size="icon" aria-label={`Fechar ${terminal.title}`} aria-haspopup="dialog" className="absolute inset-y-0 right-0.5 z-10 my-auto size-5 cursor-pointer opacity-0 transition-opacity group-hover/tab:opacity-100 focus-visible:opacity-100" onPointerDown={event => event.stopPropagation()} onClick={event => { event.stopPropagation(); setClosing(terminal); }}><X className="size-3" /></Button>
                      <RenameTerminalPopover key={`${terminal.id}:${renamingId === terminal.id ? terminal.title : "closed"}`} open={renamingId === terminal.id} terminal={terminal} pending={renamingPending} onOpenChange={next => { if (!next) setRenamingId(null); }} onRename={title => rename(terminal, title)} />
                    </div>)}
                  </TabsList>
                  <Button type="button" variant="outline" size="icon" aria-label="Novo terminal" title="Novo terminal" className="size-7 shrink-0 cursor-pointer" disabled={creating} onClick={() => { void create(); }}><Plus className="size-3.5" /></Button>
                </div>
              </div>
              {active && <TabsContent value={active.id} className="flex min-h-0 min-w-0 flex-col overflow-hidden bg-sidebar" aria-label={active.title}>
                {active.status !== "running" && <div role="status" className="flex items-center justify-between gap-3 border-b border-border bg-card px-4 py-2 text-xs text-muted-foreground"><span>Processo encerrado{active.exitCode !== null ? ` · código ${active.exitCode}` : ""}</span><Button type="button" variant="outline" size="sm" disabled={creating} onClick={() => void create()} className="h-7 cursor-pointer text-xs"><Plus className="size-3.5" />Abrir novo terminal</Button></div>}
                <div className="min-h-0 flex-1"><Suspense fallback={<Skeleton role="status" aria-label="Carregando terminal" className="h-full w-full" />}><TerminalSurface conversationId={conversationId ?? ""} terminal={active} /></Suspense></div>
              </TabsContent>}
            </Tabs>}
        </div>
        {active && <div className="flex shrink-0 items-center gap-2 border-t border-border bg-secondary/40 px-5 py-2 font-mono text-[11px] text-muted-foreground"><Folder className="size-3 shrink-0 text-onedark-cyan" /><span className="min-w-0 flex-1 truncate" title={active.command ? `${active.cwd}\n${active.command}` : active.cwd}>{active.cwd}</span><span className={`shrink-0 ${active.status === "failed" ? "text-destructive" : active.status === "running" ? "text-onedark-green" : ""}`}>{TERMINAL_STATUS_LABELS[active.status]}</span></div>}
      </section>
      </ResizablePanel>
    </ResizablePanelGroup>
    <AlertDialog open={closing !== null} onOpenChange={next => { if (!next && !closingPending) setClosing(null); }}>
      <ConfirmationDialogContent aria-describedby={undefined}>
        <AlertDialogHeader><AlertDialogTitle>Fechar {closing?.title}?</AlertDialogTitle><AlertDialogDescription>O shell e os processos iniciados por este terminal serão encerrados.</AlertDialogDescription></AlertDialogHeader>
        <AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={closingPending}>Cancelar</AlertDialogCancel><AlertDialogAction data-confirm-action className="cursor-pointer" disabled={closingPending} onClick={() => { void close(); }}>Fechar terminal</AlertDialogAction></AlertDialogFooter>
      </ConfirmationDialogContent>
    </AlertDialog>
  </>;
}
