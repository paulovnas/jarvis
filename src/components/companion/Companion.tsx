import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Activity, ArrowUpRight, CheckCheck, ChevronDown, Clock3, Gauge, MessageCircle, RefreshCw, Wifi } from "lucide-react";
import { z } from "zod";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { Progress } from "@/components/ui/progress";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { companionGeometrySchema, companionItemKey, companionSnapshotSchema, companionStatusLabels, type CompanionGeometry, type CompanionItem, type CompanionSnapshot, type CompanionStatus } from "@/core/companion";
import { accountUsageSchema, aliasSuffix, quotaPercent, remainingTime, type AccountUsage } from "@/core/provider-usage";
import type { PendingQuestion, QuestionDraft, QuestionResponse } from "@/core/questions";
import { ROLE_LABELS } from "@/core/workflow";
import { libraryError } from "@/core/library";
import { executionDuration, formatExecutionDuration, useRunningClock } from "@/hooks/use-running-clock";
import { Robot } from "./Robot";
import { CompanionChatPane } from "./CompanionChatPane";
import { CompanionQuestion, type CompanionQuestionContext } from "./CompanionQuestion";
import { useCompanionNotices } from "./use-companion-notices";
import { useCompanionStroll } from "./use-companion-stroll";
import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import "./companion.css";

const compactGeometry: CompanionGeometry = { expanded: false, bubble: false, robotSide: "right", robotVertical: "bottom", width: 96, height: 112 };
const companionError = (cause: unknown, fallback: string) => typeof cause === "string" ? cause : libraryError(cause, fallback);
const roleLabel = (role: string) => ROLE_LABELS[role as keyof typeof ROLE_LABELS] ?? role;
const statusColor: Record<CompanionStatus, string> = {
  running: "text-primary", waiting: "text-onedark-yellow", reconnecting: "text-onedark-yellow",
  completed: "text-onedark-green", failed: "text-onedark-red", idle: "text-muted-foreground",
};
const usageSchema = z.array(accountUsageSchema);

function Usage({ accounts, error, now }: { accounts: AccountUsage[] | null; error: string | null; now: number }) {
  if (error && !accounts) return <p role="alert" className="text-xs text-onedark-yellow">{error}</p>;
  if (!accounts) return <div role="status" aria-label="Carregando limites" className="space-y-4"><Skeleton className="h-4 w-36" /><Skeleton className="h-2" /><Skeleton className="h-4 w-32" /><Skeleton className="h-2" /></div>;
  return <div className="space-y-4">
    {error && <p role="status" className="text-xs text-onedark-yellow">{error}</p>}
    {!accounts.length && <p className="py-8 text-center text-xs text-muted-foreground">Nenhum limite disponível nos provedores conectados.</p>}
    {accounts.map(account => <section key={account.alias} aria-label={`Limites de ${account.alias}`} className="space-y-3 rounded-md border border-border bg-sidebar/40 p-3">
      <p className="truncate text-xs font-medium">{aliasSuffix(account.alias)}</p>
      {account.error && <p role="status" className="text-[11px] text-onedark-yellow">{account.fetchedAt ? "Limites desatualizados" : "Limites indisponíveis"}</p>}
      {!account.error && !account.windows.length && <p className="text-[11px] text-muted-foreground">Nenhuma janela informada.</p>}
      {account.windows.map(window => <div key={window.id} className="space-y-1.5">
        <div className="flex items-center justify-between gap-3 font-mono text-[10px]"><span className="truncate text-muted-foreground">{window.group} · {window.label}</span><span className={window.remainingPercent !== null && window.remainingPercent <= 10 ? "text-onedark-red" : window.remainingPercent !== null && window.remainingPercent <= 30 ? "text-onedark-yellow" : "text-onedark-green"}>{quotaPercent(window.remainingPercent)} restante</span></div>
        {window.remainingPercent !== null && <Progress aria-label={`${window.group} ${window.label} restante`} value={window.remainingPercent} className={window.remainingPercent <= 10 ? "[&_[data-slot=progress-indicator]]:bg-onedark-red" : window.remainingPercent <= 30 ? "[&_[data-slot=progress-indicator]]:bg-onedark-yellow" : "[&_[data-slot=progress-indicator]]:bg-onedark-green"} />}
        {window.resetsAt !== null && <p className="font-mono text-[9px] text-muted-foreground">{remainingTime(window.resetsAt, now) === "agora" ? "Reset previsto agora" : `Renova em ${remainingTime(window.resetsAt, now)}`}</p>}
      </div>)}
      {account.fetchedAt && <p className="font-mono text-[9px] text-muted-foreground/70">Atualizado às {new Date(account.fetchedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</p>}
    </section>)}
  </div>;
}

export function Companion() {
  const [geometry, setGeometry] = useState<CompanionGeometry>(compactGeometry);
  const [snapshot, setSnapshot] = useState<CompanionSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [tab, setTab] = useState("activity");
  const [usage, setUsage] = useState<AccountUsage[] | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [usageAt, setUsageAt] = useState(() => Date.now());
  const [usageAttempt, setUsageAttempt] = useState(0);
  const [usageBusy, setUsageBusy] = useState(false);
  const forceUsage = useRef(false);
  const [resizing, setResizing] = useState(false);
  const [closing, setClosing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [gaze, setGaze] = useState({ x: 0, y: 0 });
  const [visible, setVisible] = useState(() => !document.hidden);
  const robotElement = useRef<HTMLButtonElement>(null);
  const resizeLock = useRef(false);
  const pendingCollapse = useRef(false);
  const collapseHandler = useRef<() => void>(() => {});
  const geometryRevision = useRef(0);
  const pointer = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  const dragged = useRef(false);
  const [drafts] = useState(() => new Map<string, QuestionDraft>());
  const [chatQuestion, setChatQuestion] = useState<CompanionQuestionContext | null>(null);
  const { notice, sync: syncNotices, clear: clearNotices } = useCompanionNotices();
  const refreshSnapshot = useRef<() => void>(() => {});
  const acknowledgements = useRef(new Set<string>());
  const items = snapshot?.items ?? [];
  const waiting = items.filter(item => item.status === "waiting").length;
  const working = items.filter(item => item.status === "running" || item.status === "reconnecting").length;
  const current = items.find(item => item.status === "waiting") ?? items.find(item => item.status === "reconnecting")
    ?? items.find(item => item.status === "running") ?? items.reduce<CompanionItem | undefined>((latest, item) => !latest || item.updatedAt > latest.updatedAt ? item : latest, undefined);
  const focused = items.find(item => companionItemKey(item) === selectedKey) ?? current;
  const attentive = items.filter(item => item.status !== "idle" && (!(item.status === "completed" || item.status === "failed") || !item.acknowledged));
  const petCurrent = attentive.find(item => item.status === "waiting") ?? attentive.find(item => item.status === "reconnecting")
    ?? attentive.find(item => item.status === "running") ?? attentive.reduce<CompanionItem | undefined>((latest, item) => !latest || item.updatedAt > latest.updatedAt ? item : latest, undefined);
  const status: CompanionStatus = error && !snapshot ? "failed" : petCurrent?.status ?? "idle";
  const snapshotQuestion = focused?.status === "waiting" && (focused.pendingQuestion || focused.requiresConversation) ? focused : items.find(item => item.status === "waiting" && (item.pendingQuestion || item.requiresConversation));
  const question = useMemo<CompanionQuestionContext | null>(() => tab === "chat" && chatQuestion ? chatQuestion : snapshotQuestion ? {
    conversationId: snapshotQuestion.conversationId, agentId: snapshotQuestion.agentId, title: snapshotQuestion.title,
    projectName: snapshotQuestion.projectName, request: snapshotQuestion.pendingQuestion, requiresConversation: snapshotQuestion.requiresConversation,
  } : null, [tab, chatQuestion, snapshotQuestion]);
  const bubbleWanted = Boolean(notice && !geometry.expanded);
  const bubbleVisible = bubbleWanted && geometry.bubble && !dragging;
  const now = useRunningClock(geometry.expanded && items.some(item => (item.status === "running" || item.status === "reconnecting") && item.activeSince !== null));
  const strolling = geometry.expanded && visible && !closing && !dragging && (status === "running" || status === "idle");
  const stroll = useCompanionStroll(robotElement, strolling);
  const facing = geometry.robotSide === "left" ? -stroll.facing : stroll.facing;

  useEffect(() => {
    const changed = () => setVisible(!document.hidden);
    document.addEventListener("visibilitychange", changed);
    return () => document.removeEventListener("visibilitychange", changed);
  }, []);

  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    let inFlight = false;
    let dirty = false;
    const schedule = () => {
      if (!alive || timer !== undefined) return;
      timer = window.setTimeout(() => { timer = undefined; void refresh(); }, 100);
    };
    const refresh = async () => {
      if (inFlight) { dirty = true; return; }
      inFlight = true;
      try {
        const next = companionSnapshotSchema.parse(await invoke("get_companion_snapshot"));
        if (alive) { syncNotices(next.items); setSnapshot(next); setError(null); }
      } catch (cause) { if (alive) setError(companionError(cause, "Não foi possível carregar as atividades.")); }
      finally {
        inFlight = false;
        if (alive && dirty) { dirty = false; schedule(); }
      }
    };
    refreshSnapshot.current = schedule;
    const subscriptions = [
      listen("companion:changed", schedule),
      listen("companion:geometry", event => {
        const next = companionGeometrySchema.safeParse(event.payload);
        if (alive && next.success) { geometryRevision.current++; setGeometry(next.data); }
      }),
      listen("companion:drag-end", () => { pointer.current = null; if (alive) setDragging(false); }),
      listen("companion:collapse-request", () => { if (alive) collapseHandler.current(); }),
    ];
    void Promise.all(subscriptions).then(() => { if (alive) void refresh(); }).catch(cause => { if (alive) setError(companionError(cause, "Não foi possível acompanhar as atividades.")); });
    return () => { alive = false; refreshSnapshot.current = () => {}; window.clearTimeout(timer); for (const subscription of subscriptions) void subscription.then(stop => stop()).catch(() => {}); };
  }, [attempt, syncNotices]);

  useEffect(() => {
    let alive = true;
    const revision = ++geometryRevision.current;
    void invoke("set_companion_expanded", { expanded: false }).then(value => {
      const next = companionGeometrySchema.safeParse(value);
      if (alive && revision === geometryRevision.current && next.success) setGeometry(next.data);
    }).catch(cause => { if (alive) setError(companionError(cause, "Não foi possível posicionar o assistente.")); });
    return () => { alive = false; void invoke("companion_set_interacting", { active: false }).catch(() => {}); };
  }, []);

  useEffect(() => {
    if (dragging || resizing || bubbleWanted === geometry.bubble) return;
    let alive = true;
    const revision = ++geometryRevision.current;
    void invoke("set_companion_bubble", { visible: bubbleWanted }).then(value => {
      const next = companionGeometrySchema.safeParse(value);
      if (alive && revision === geometryRevision.current && next.success) setGeometry(next.data);
    }).catch(cause => { if (alive) setError(companionError(cause, "Não foi possível mostrar o aviso.")); });
    return () => { alive = false; };
  }, [bubbleWanted, geometry.bubble, dragging, resizing]);

  const acknowledge = useCallback(async (item: CompanionItem) => {
    if (item.acknowledged || item.status !== "completed" && item.status !== "failed" || acknowledgements.current.has(item.attentionId)) return;
    acknowledgements.current.add(item.attentionId);
    try {
      await invoke("ack_companion_item", { conversationId: item.conversationId, agentId: item.agentId, attentionId: item.attentionId });
      refreshSnapshot.current();
    } catch (cause) {
      acknowledgements.current.delete(item.attentionId);
      setError(companionError(cause, "Não foi possível marcar a atividade como vista."));
    }
  }, []);
  useEffect(() => {
    if (!geometry.expanded || !visible || tab !== "activity" || question || !focused) return;
    const afterPaint = window.setTimeout(() => { void acknowledge(focused); }, 0);
    return () => window.clearTimeout(afterPaint);
  }, [geometry.expanded, visible, tab, question, focused, acknowledge]);
  const viewedChat = useCallback((conversationId: string) => {
    if (!visible || question) return;
    const item = snapshot?.items.find(item => item.conversationId === conversationId && item.agentId === null);
    if (item) void acknowledge(item);
  }, [visible, question, snapshot, acknowledge]);

  useEffect(() => {
    if (!geometry.expanded || tab !== "usage") return;
    let alive = true;
    let version = 0;
    const force = forceUsage.current; forceUsage.current = false;
    const refresh = async (force = false) => {
      const request = ++version;
      if (force) setUsageBusy(true);
      try {
        const next = usageSchema.parse(await invoke("get_companion_usage", force ? { refresh: true } : undefined));
        if (alive && request === version) { setUsage(next); setUsageError(null); setUsageAt(Date.now()); }
      } catch (cause) { if (alive && request === version) setUsageError(companionError(cause, "Não foi possível carregar os limites.")); }
      finally { if (alive) setUsageBusy(false); }
    };
    const subscription = listen("companion:usage", () => { void refresh(); });
    void subscription.then(() => { if (alive) void refresh(force); }).catch(cause => { if (alive) setUsageError(companionError(cause, "Não foi possível acompanhar os limites.")); });
    return () => { alive = false; void subscription.then(stop => stop()).catch(() => {}); };
  }, [geometry.expanded, tab, usageAttempt]);

  const expand = useCallback(async (expanded: boolean) => {
    if (resizeLock.current) { if (!expanded) pendingCollapse.current = true; return; }
    resizeLock.current = true; setResizing(true);
    geometryRevision.current++;
    try {
      if (!expanded && geometry.expanded && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        const placement = robotElement.current?.parentElement;
        if (placement) placement.style.setProperty("--robot-return-transform", window.getComputedStyle(placement).transform);
        setClosing(true);
        await new Promise(resolve => window.setTimeout(resolve, 220));
      }
      const revision = ++geometryRevision.current;
      const next = companionGeometrySchema.parse(await invoke("set_companion_expanded", { expanded }));
      if (revision === geometryRevision.current) setGeometry(next);
    }
    catch (cause) { setError(companionError(cause, "Não foi possível abrir o assistente.")); }
    finally { resizeLock.current = false; setResizing(false); setClosing(false); }
  }, [geometry.expanded]);
  useEffect(() => {
    collapseHandler.current = () => { void expand(false); };
  }, [expand]);
  useEffect(() => {
    if (!resizing && pendingCollapse.current) { pendingCollapse.current = false; void expand(false); }
  }, [resizing, expand]);
  const openConversation = async (target: { conversationId: string; agentId?: string | null }) => {
    const accessed = items.find(item => item.conversationId === target.conversationId && item.agentId === (target.agentId ?? null));
    try {
      await invoke("companion_open_conversation", { conversationId: target.conversationId });
      if (accessed) await acknowledge(accessed);
      refreshSnapshot.current();
    }
    catch (cause) { setError(companionError(cause, "Não foi possível abrir a conversa.")); }
  };
  const answerQuestion = async (request: PendingQuestion, response: QuestionResponse) => {
    if (!question) return false;
    try {
      const result = await invoke("companion_answer_question", { conversationId: question.conversationId, agentId: question.agentId, turnId: request.turnId, toolId: request.toolId, response });
      if (result === false) { setError("Esta pergunta já foi respondida. Aguardando atualização…"); return false; }
      return true;
    } catch (cause) { setError(companionError(cause, "Não foi possível enviar a resposta.")); return false; }
  };
  const pauseQuestion = async (request: PendingQuestion) => {
    if (!question) return false;
    try { return await invoke("companion_pause_question", { conversationId: question.conversationId, agentId: question.agentId, turnId: request.turnId, toolId: request.toolId }) !== false; }
    catch (cause) { setError(companionError(cause, "Não foi possível pausar a resposta automática.")); return false; }
  };
  const activeDuration = (item: CompanionItem) => formatExecutionDuration(executionDuration(item.updatedAt, item.durationMs, item.status === "running" || item.status === "reconnecting", now, item.activeSince));

  const pet = <div className="companion-robot-placement">
    <Button ref={robotElement} variant="ghost" className="companion-pet relative h-28 w-24 cursor-pointer p-0 focus-visible:ring-2 focus-visible:ring-onedark-cyan/60" aria-label={geometry.expanded ? "Recolher assistente Jarvis" : `Abrir assistente Jarvis${waiting ? `, ${waiting} interações pendentes` : working ? `, ${working} atividades em andamento` : ""}`} aria-expanded={geometry.expanded} aria-controls="companion-panel" disabled={resizing}
      onPointerEnter={() => setHovered(true)} onPointerLeave={() => setHovered(false)}
      onPointerDown={event => { if (event.button !== 0) return; event.currentTarget.setPointerCapture?.(event.pointerId); pointer.current = { x: event.clientX, y: event.clientY, dragging: false }; dragged.current = false; }}
      onPointerMove={event => {
        if (!(event.buttons & 1)) { pointer.current = null; return; }
        const start = pointer.current;
        if (!start || start.dragging || Math.hypot(event.clientX - start.x, event.clientY - start.y) < 5) return;
        start.dragging = true; dragged.current = true;
        geometryRevision.current++; setDragging(true);
        const rect = robotElement.current?.getBoundingClientRect();
        void invoke("companion_start_drag", rect ? { robotX: rect.x, robotY: rect.y } : undefined).catch(cause => { setDragging(false); setError(companionError(cause, "Não foi possível mover o assistente.")); });
      }}
      onPointerUp={event => { pointer.current = null; if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); }} onPointerCancel={() => { pointer.current = null; }} onLostPointerCapture={() => { pointer.current = null; }}
      onClick={event => { if (dragged.current && event.detail !== 0) { dragged.current = false; return; } clearNotices(); void expand(!geometry.expanded); }}>
      <span className="companion-robot-facing" data-facing={strolling ? facing : 1}>
        <Robot status={status} visible={visible} hovered={hovered} dragging={dragging} expanded={geometry.expanded} walking={stroll.walking && !hovered} lookX={gaze.x * (strolling ? facing : 1)} lookY={gaze.y} />
      </span>
      {(waiting > 0 || working > 0 || status === "failed") && <Badge aria-hidden="true" className={`absolute right-1 bottom-4 min-w-5 justify-center rounded-full border border-sidebar px-1.5 py-0.5 font-mono text-[9px] ${waiting || status === "failed" ? "bg-onedark-yellow text-sidebar" : "bg-onedark-cyan text-sidebar"}`}>{waiting || working || "!"}</Badge>}
    </Button>
  </div>;

  const panel = <Card id="companion-panel" role="region" aria-label="Assistente Jarvis" hidden={!geometry.expanded} className={`companion-panel min-h-0 flex-1 gap-0 rounded-none border-0 bg-transparent p-0 text-foreground shadow-none ring-0 ${geometry.expanded ? "" : "hidden"}`} onPointerDownCapture={event => { if (question && event.target instanceof Element && event.target.closest("input, textarea, button, [contenteditable=true]")) void invoke("companion_set_interacting", { active: true }).catch(() => {}); }} onBlurCapture={event => { if (!(event.relatedTarget instanceof HTMLElement && event.currentTarget.contains(event.relatedTarget))) void invoke("companion_set_interacting", { active: false }).catch(() => {}); }}>
    <header className="flex shrink-0 items-center gap-2 px-5 pb-2 pt-5">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-onedark-cyan" />
      <h1 className="text-sm font-semibold tracking-tight text-foreground">Jarvito</h1>
      <span role="status" className={`ml-1 min-w-0 flex-1 truncate text-[11px] ${statusColor[status]}`}>{snapshot ? companionStatusLabels[status] : "Carregando…"}</span>
      <Hint content="Recolher assistente"><Button size="icon-xs" variant="ghost" className="cursor-pointer text-muted-foreground" aria-label="Recolher painel" disabled={resizing} onClick={() => { void expand(false); }}><ChevronDown className="size-3.5" /></Button></Hint>
    </header>
    {geometry.expanded && question && <CompanionQuestion context={question} drafts={drafts} onAnswer={answerQuestion} onInteract={pauseQuestion} onOpenConversation={() => { void openConversation(question); }} error={error} />}
    <Tabs hidden={Boolean(geometry.expanded && question)} value={tab} onValueChange={value => { if (typeof value === "string") setTab(value); }} className={`min-h-0 flex-1 gap-0 ${geometry.expanded && question ? "hidden" : ""}`}>
      <TabsList aria-label="Recursos do assistente" variant="line" className="mx-4 mt-1 h-9 w-auto shrink-0 justify-start gap-3">
        <TabsTrigger value="chat" className="flex-none cursor-pointer text-[11px]"><MessageCircle className="size-3.5" />Chat</TabsTrigger>
        <TabsTrigger value="activity" className="flex-none cursor-pointer text-[11px]"><Activity className="size-3.5" />Atividade{working > 0 && <Badge variant="outline" className="ml-1 px-1 py-0 font-mono text-[9px]">{working}</Badge>}</TabsTrigger>
        <TabsTrigger value="usage" className="flex-none cursor-pointer text-[11px]"><Gauge className="size-3.5" />Limites</TabsTrigger>
      </TabsList>
      <TabsContent value="activity" className="min-h-0 overflow-hidden px-3 pb-3 pt-2" onPointerDownCapture={event => { if (event.target instanceof Element && event.target.closest("input, textarea, [contenteditable=true], [data-slot=select-trigger]")) void invoke("companion_set_interacting", { active: true }).catch(() => {}); }} onBlurCapture={event => { if (!(event.relatedTarget instanceof HTMLElement && event.currentTarget.contains(event.relatedTarget))) void invoke("companion_set_interacting", { active: false }).catch(() => {}); }}>
        <ScrollArea className="h-full min-h-0 pr-2">
          {error && <div className="mb-3 space-y-2"><p role="alert" className="text-xs text-onedark-yellow">{error}</p><Button size="sm" variant="outline" className="h-7 cursor-pointer text-[11px]" onClick={() => setAttempt(value => value + 1)}>Tentar novamente</Button></div>}
          {!snapshot && !error && <div role="status" aria-label="Carregando atividades" className="space-y-3"><Skeleton className="h-4 w-28" /><Skeleton className="h-12" /><Skeleton className="h-16" /></div>}
          {snapshot && !items.length && <div className="flex flex-col items-center gap-3 py-10 text-center"><CheckCheck className="size-5 text-onedark-cyan/70" /><p className="text-xs font-medium">Tudo tranquilo por aqui</p><p className="max-w-64 text-[11px] leading-5 text-muted-foreground">Atividades, perguntas e conclusões aparecem aqui enquanto você usa outros aplicativos.</p></div>}
          {items.length > 1 && <Select value={focused ? companionItemKey(focused) : undefined} onValueChange={value => { if (typeof value === "string") setSelectedKey(value); }}>
            <SelectTrigger aria-label="Conversa ou agente" size="sm" className="mb-3 w-full cursor-pointer text-[11px]"><SelectValue><span className="truncate">{focused?.title}</span></SelectValue></SelectTrigger>
            <SelectContent className="max-h-64 max-w-[calc(100vw-24px)]">{items.map(item => <SelectItem key={companionItemKey(item)} value={companionItemKey(item)} className="cursor-pointer py-2 text-[11px]">
              <span className="flex min-w-0 flex-col gap-1"><span className="truncate font-medium">{item.title}</span><span className="truncate font-mono text-[9px] text-muted-foreground">{item.projectName} · {roleLabel(item.role)}{item.agentId ? " · subagente" : ""} · {companionStatusLabels[item.status]}</span></span>
            </SelectItem>)}</SelectContent>
          </Select>}
          {focused && <section aria-label="Atividade selecionada" className="space-y-3">
            <div className="flex items-start gap-2"><div className="min-w-0 flex-1"><p className="truncate font-mono text-[9px] text-muted-foreground">{focused.projectName} · {roleLabel(focused.role)}{focused.agentId ? " · subagente" : ""}</p><h2 className="mt-1 break-words text-sm leading-5 font-medium">{focused.title}</h2></div><Hint content="Abrir conversa no Jarvis"><Button size="icon-xs" variant="ghost" aria-label="Abrir conversa no Jarvis" className="shrink-0 cursor-pointer text-muted-foreground" onClick={() => { void openConversation(focused); }}><ArrowUpRight className="size-3.5" /></Button></Hint></div>
            <div className="flex items-center gap-2"><Badge variant="outline" className={`text-[10px] ${statusColor[focused.status]}`}>{focused.status === "reconnecting" && <Wifi className="mr-1 size-3" />}{companionStatusLabels[focused.status]}</Badge><span className="flex items-center gap-1 font-mono text-[10px] text-muted-foreground"><Clock3 aria-hidden="true" className="size-3" />{activeDuration(focused)}</span></div>
            {focused.activity && <p className="break-words rounded-md border border-border bg-sidebar/50 px-3 py-2 text-[11px] leading-5 text-muted-foreground">{focused.activity}</p>}
            {focused.requiresConversation && !focused.pendingQuestion && <Button size="sm" variant="secondary" className="w-full cursor-pointer text-xs" onClick={() => { void openConversation(focused); }}><MessageCircle className="size-3.5" />Continuar no Jarvis</Button>}
          </section>}
          {snapshot?.truncated && <p className="mt-3 text-[10px] text-muted-foreground">As atividades mais recentes estão aqui. Veja as demais no Jarvis.</p>}
        </ScrollArea>
      </TabsContent>
      <TabsContent value="chat" keepMounted className="min-h-0 flex-1 overflow-hidden px-4 pb-1 pt-2" onPointerDownCapture={event => { if (event.target instanceof Element && event.target.closest("input, textarea, [contenteditable=true], [data-slot=select-trigger], [aria-haspopup=menu]")) void invoke("companion_set_interacting", { active: true }).catch(() => {}); }}>
        <CompanionChatPane active={geometry.expanded && tab === "chat"} externalQuestions onQuestionChange={setChatQuestion} onView={viewedChat} />
      </TabsContent>
      <TabsContent value="usage" className="min-h-0 overflow-hidden px-3 pb-3 pt-2"><div className="mb-3 flex items-center justify-between gap-2"><p className="text-[10px] text-muted-foreground">Cotas disponíveis dos provedores</p><Hint content="Atualizar limites"><Button aria-label="Atualizar limites" aria-busy={usageBusy} disabled={usageBusy} size="icon-xs" variant="ghost" className="cursor-pointer text-muted-foreground" onClick={() => { forceUsage.current = true; setUsageAttempt(value => value + 1); }}><RefreshCw className={`size-3 ${usageBusy ? "motion-safe:animate-spin" : ""}`} /></Button></Hint></div><ScrollArea className="h-[calc(100%-34px)] pr-2"><Usage accounts={usage} error={usageError} now={usageAt} /></ScrollArea></TabsContent>
    </Tabs>
  </Card>;

  const speech = bubbleVisible && notice && <Card role="status" aria-label="Aviso do Jarvito" className="companion-speech min-h-0 gap-2 overflow-hidden rounded-xl border-border bg-card p-3 shadow-lg">
    <p className="truncate font-mono text-[9px] text-muted-foreground">{notice.item.projectName} · {roleLabel(notice.item.role)}</p>
    <p className="line-clamp-2 break-words text-xs leading-4 font-medium">{notice.item.status === "waiting" ? notice.item.pendingQuestion?.questions[0]?.question ?? "Preciso de uma decisão sua para continuar." : notice.item.status === "completed" ? "Sua resposta está pronta." : "Não consegui concluir esta solicitação."}</p>
    {notice.item.status !== "waiting" && <div className="companion-speech-result line-clamp-3 text-[11px] leading-4 text-muted-foreground"><LazyChatMarkdown content={notice.item.status === "completed" ? notice.item.result || (notice.item.title === "Jarvito" ? "Abra o chat para ver a resposta." : notice.item.title) : notice.item.activity || "Abra o chat para ver o que aconteceu."} /></div>}
    <Button size="sm" variant="ghost" className="h-7 shrink-0 cursor-pointer justify-start px-1 text-[10px] text-primary" onClick={() => { setSelectedKey(companionItemKey(notice.item)); setTab("activity"); clearNotices(); void expand(true); }}>{notice.item.status === "waiting" ? notice.item.pendingQuestion && !notice.item.requiresConversation ? "Responder pergunta" : "Ver solicitação" : "Ver atividade"}<ArrowUpRight className="size-3" /></Button>
  </Card>;

  return <main className="companion dark h-full min-h-0 w-full text-foreground" data-status={status} data-expanded={geometry.expanded} data-bubble={bubbleVisible} data-robot-side={geometry.robotSide} data-robot-vertical={geometry.robotVertical} data-closing={closing} data-visible={visible} onPointerMove={event => {
    const rect = robotElement.current?.getBoundingClientRect();
    if (rect && event.buttons === 0) {
      const x = Math.round(Math.tanh((event.clientX - rect.x - rect.width / 2) / 100) * 10) / 10;
      const y = Math.round(Math.tanh((event.clientY - rect.y - rect.height / 2) / 100) * 10) / 10;
      setGaze(previous => previous.x === x && previous.y === y ? previous : { x, y });
      robotElement.current?.style.setProperty("--robot-look-x", `${x * 4}px`);
      robotElement.current?.style.setProperty("--robot-look-y", `${y * 3}px`);
    }
  }} onPointerLeave={() => { setHovered(false); setGaze(previous => previous.x === 0 && previous.y === 0 ? previous : { x: 0, y: 0 }); robotElement.current?.style.setProperty("--robot-look-x", "0px"); robotElement.current?.style.setProperty("--robot-look-y", "0px"); }} onKeyDown={event => { if (event.key === "Escape" && geometry.expanded) { event.preventDefault(); void expand(false); } }}>
    <div className="companion-island">{panel}{speech}{pet}</div>
  </main>;
}
