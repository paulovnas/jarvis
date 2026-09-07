import { memo, useLayoutEffect, useRef, useState } from "react";
import { ArrowDown, MessageSquare } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import type { AgentTurn, ChatSnapshot, HistoryExcerpt } from "@/core/chat";
import { historyWindow, type HistoryDirection } from "@/core/chat-history";
import type { ChatController } from "@/hooks/use-chat";
import { AssistantMessageTurn } from "./AssistantMessageTurn";
import { UserMessageBubble } from "./UserMessageBubble";
import { CompactionMarker } from "./CompactionMarker";

export const TurnBody = memo(function TurnBody({ turn }: { turn: AgentTurn }) {
  const timestamp = new Date(turn.createdAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
  return <AssistantMessageTurn message={{
    id: turn.id, role: "assistant", content: turn.steps[turn.steps.length - 1]?.text ?? "", timestamp,
    model: `${turn.options.account} / ${turn.options.model}`, streaming: turn.status === "running",
    work: turn.status === "running" || turn.steps.some((step, index) => step.summary || step.tools.length || (step.text && index < turn.steps.length - 1)) ? {
      retry: turn.status === "running" ? turn.steps[turn.steps.length - 1]?.retry : undefined,
      durationSeconds: Math.round(turn.durationMs / 1000), steps: turn.steps.map((step, index) => ({ thinking: step.summary, tools: step.tools, commentary: index < turn.steps.length - 1 ? step.text : "" })),
    } : undefined,
    error: turn.error ? { title: turn.status === "cancelled" || turn.status === "interrupted" ? "Execução interrompida" : "Falha na execução", message: turn.error.message } : undefined,
  }} />;
});

function HistoryRail({ entries, total, active, disabled, jump }: { entries: HistoryExcerpt[]; total: number; active: number; disabled: boolean; jump: (index: number) => void }) {
  if (entries.length < 10) return null;
  const selected = [...entries].reverse().find(entry => entry.index <= active)?.id;
  return <nav aria-label="Navegar pela conversa" style={{ maxHeight: `min(calc(100% - 32px), ${entries.length * Math.max(8, 18 - entries.length / 4)}px)` }} className="group/rail absolute top-1/2 left-0 z-10 flex w-3.5 -translate-y-1/2 flex-col items-center overflow-y-auto transition-[width] hover:w-7 focus-within:w-7 motion-reduce:transition-none">
    <TooltipProvider delay={150}>{entries.map(entry => <Tooltip key={entry.id}>
      <TooltipTrigger render={<Button variant="ghost" size="icon" />} disabled={disabled} aria-label={`Ir para interação ${entry.index + 1}: ${entry.user}`} aria-current={selected === entry.id ? "location" : undefined} onClick={() => jump(entry.index)} className="group h-4 min-h-2 w-full shrink cursor-pointer rounded-sm px-0 py-1 hover:bg-primary/10 focus-visible:bg-primary/10">
        <span aria-hidden="true" className={`block h-px rounded-full transition-[width,background-color] group-hover/rail:w-3 group-focus-within/rail:w-3 group-hover:w-5! group-focus-visible:w-5! motion-reduce:transition-none ${selected === entry.id ? "w-2 bg-primary shadow-[0_0_6px_#61afef66]" : "w-1 bg-muted-foreground/45 group-hover:bg-foreground"}`} />
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={8} className="block w-72 max-w-[min(288px,70vw)] space-y-2 border border-border bg-card p-3 text-foreground shadow-xl">
        <p className="font-mono text-[10px] tabular-nums text-muted-foreground">{entry.index + 1} / {total} <span className="float-right">{new Date(entry.createdAt).toLocaleDateString("pt-BR")}</span></p>
        <p className="line-clamp-2 text-xs font-medium">{entry.user}</p>
        {entry.assistant && <p className="line-clamp-3 border-t border-border pt-2 text-xs leading-relaxed text-muted-foreground">{entry.assistant}</p>}
      </TooltipContent>
    </Tooltip>)}</TooltipProvider>
  </nav>;
}

export function Transcript({ snapshot, chat }: { snapshot: ChatSnapshot; chat: ChatController }) {
  const root = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  const inFlight = useRef(false);
  const restoring = useRef(false);
  const lastScroll = useRef(0);
  const anchor = useRef<{ id?: string; offset: number; target?: number; latest?: boolean; ready: boolean } | null>(null);
  const [restoreVersion, setRestoreVersion] = useState(0);
  const [visible, setVisible] = useState<number | null>(null);
  const window = historyWindow(snapshot);
  const hasOlder = window.start > 0;
  const hasNewer = window.start + snapshot.turns.length < window.total;
  const viewport = () => root.current?.querySelector<HTMLElement>('[data-slot="scroll-area-viewport"]');
  const rows = () => Array.from(root.current?.querySelectorAll<HTMLElement>('[data-turn-id]') ?? []);
  const navigate = async (direction: HistoryDirection) => {
    if (inFlight.current) return;
    const view = viewport();
    const top = view?.getBoundingClientRect().top ?? 0;
    const row = rows().find(row => row.getBoundingClientRect().bottom > top);
    anchor.current = { id: row?.dataset.turnId, offset: (row?.getBoundingClientRect().top ?? top) - top, target: typeof direction === "number" ? direction : undefined, latest: direction === "latest", ready: false };
    follow.current = false; inFlight.current = true;
    const ok = await chat.loadHistory(direction);
    inFlight.current = false;
    if (ok && anchor.current) { anchor.current.ready = true; setRestoreVersion(version => version + 1); }
    else anchor.current = null;
  };
  useLayoutEffect(() => {
    const view = viewport(); if (!view) return;
    const saved = anchor.current;
    if (saved?.ready) {
      restoring.current = true;
      const row = rows().find(row => saved.target !== undefined ? Number(row.dataset.turnIndex) === saved.target : row.dataset.turnId === saved.id);
      if (saved.latest) { view.scrollTop = view.scrollHeight; follow.current = true; }
      else if (row) view.scrollTop += row.getBoundingClientRect().top - view.getBoundingClientRect().top - (saved.target !== undefined ? 12 : saved.offset);
      lastScroll.current = view.scrollTop;
      anchor.current = null;
      requestAnimationFrame(() => {
        restoring.current = false;
        if (saved.latest) setVisible(window.total - 1);
        else if (saved.target !== undefined) setVisible(saved.target);
      });
    } else if (follow.current && !hasNewer) { view.scrollTop = view.scrollHeight; lastScroll.current = view.scrollTop; }
  }, [snapshot.turns, hasNewer, restoreVersion, window.total]);
  useLayoutEffect(() => {
    const view = viewport();
    const content = root.current?.querySelector<HTMLElement>('[aria-label="Histórico de mensagens"]');
    if (!view || !content) return;
    const observer = new ResizeObserver(() => {
      if (follow.current && !hasNewer) { view.scrollTop = view.scrollHeight; lastScroll.current = view.scrollTop; }
    });
    observer.observe(view); observer.observe(content);
    return () => observer.disconnect();
  }, [hasNewer]);

  return <div ref={root} className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
    <HistoryRail entries={snapshot.navigation ?? []} total={window.total} active={visible ?? window.total - 1} disabled={chat.historyLoading} jump={index => { void navigate(index); }} />
    <ScrollArea className="transcript-scroll h-0 min-h-0 flex-1 overflow-hidden" onScrollCapture={event => {
      const target = event.target;
      if (!(target instanceof HTMLElement) || target.dataset.slot !== "scroll-area-viewport" || restoring.current) return;
      // WebKit can emit elastic overscroll positions outside the content bounds.
      if (target.scrollTop < 0 || target.scrollTop > Math.max(0, target.scrollHeight - target.clientHeight)) return;
      const previous = lastScroll.current; lastScroll.current = target.scrollTop;
      const remaining = target.scrollHeight - target.scrollTop - target.clientHeight;
      if (!hasNewer && remaining < 80) follow.current = true;
      else if (target.scrollTop < previous) follow.current = false;
      const top = target.getBoundingClientRect().top;
      const row = rows().find(row => row.getBoundingClientRect().bottom > top + 20);
      if (row) setVisible(Number(row.dataset.turnIndex));
      if (chat.historyLoading || chat.historyError) return;
      if (hasOlder && target.scrollTop < 160 && target.scrollTop < previous) void navigate("older");
      else if (hasNewer && remaining < 160 && target.scrollTop > previous) void navigate("newer");
    }}>
      <div className="mx-auto w-full max-w-4xl min-w-0 px-5" aria-label="Histórico de mensagens">
        {hasOlder && <div className="flex justify-center py-3"><Button variant="ghost" size="sm" className="cursor-pointer text-xs text-muted-foreground" disabled={chat.historyLoading} onClick={() => { void navigate("older"); }}>Mensagens anteriores</Button></div>}
        {chat.historyLoading && <div role="status" aria-label="Carregando trecho" className="space-y-2 py-3 pl-4"><Skeleton className="h-3 w-1/3" /><Skeleton className="h-3 w-2/3" /></div>}
        {chat.historyError && <p role="alert" className="px-4 py-2 text-xs text-destructive">{chat.historyError}</p>}
        {snapshot.turns.length === 0 && <Empty className="py-16"><EmptyHeader><EmptyMedia variant="icon"><MessageSquare /></EmptyMedia><EmptyTitle>Conversa criada</EmptyTitle><EmptyDescription>Envie uma instrução para começar a trabalhar neste projeto.</EmptyDescription></EmptyHeader></Empty>}
        {snapshot.turns.map((turn, index) => <div key={turn.id} data-turn-id={turn.id} data-turn-index={window.start + index} className="[overflow-anchor:none]">
          <UserMessageBubble message={{ id: turn.id, role: "user", content: turn.user, parts: turn.parts, timestamp: new Date(turn.createdAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" }) }} />
          {snapshot.compactions?.filter(event => event.turnId === turn.id && !event.afterTurn).map(event => <CompactionMarker key={event.id} event={event} />)}
          <TurnBody turn={turn} />
          {snapshot.compactions?.filter(event => event.turnId === turn.id && event.afterTurn).map(event => <CompactionMarker key={event.id} event={event} />)}
        </div>)}
        {hasNewer && <div className="flex justify-center py-3"><Button variant="ghost" size="sm" className="cursor-pointer text-xs text-muted-foreground" disabled={chat.historyLoading} onClick={() => { void navigate("newer"); }}>Mensagens seguintes</Button></div>}
      </div>
    </ScrollArea>
    {hasNewer && <Button variant="outline" size="sm" disabled={chat.historyLoading} className="absolute bottom-3 left-1/2 -translate-x-1/2 cursor-pointer gap-2 bg-card shadow-lg" onClick={() => { void navigate("latest"); }}><ArrowDown className="size-3.5" />Voltar ao presente</Button>}
  </div>;
}
