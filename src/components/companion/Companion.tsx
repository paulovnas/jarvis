import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpRight, ChevronDown, Gauge, Home, MessageCircle, Phone, PhoneOff, RefreshCw, Settings2, Volume2, VolumeX, Wifi } from "lucide-react";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { ProviderIcon } from "@/components/ProviderIcon";
import { WindowBar } from "@/components/layout/ProviderUsage";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { companionGeometrySchema, companionItemKey, companionSnapshotSchema, companionStatusLabels, type CompanionGeometry, type CompanionItem, type CompanionSnapshot, type CompanionStatus } from "@/core/companion";
import { accountUsageSchema, aliasSuffix } from "@/core/provider-usage";
import type { PendingQuestion, QuestionDraft, QuestionResponse } from "@/core/questions";
import { ROLE_LABELS } from "@/core/workflow";
import { libraryError } from "@/core/library";
import { executionDuration, formatExecutionDuration, useRunningClock } from "@/hooks/use-running-clock";
import { Robot, type RobotProps } from "./Robot";
import { useRobotRest } from "./use-robot-rest";
import { CompanionChatPane, type CompanionChatHandle } from "./CompanionChatPane";
import { CompanionQuestion, type CompanionQuestionContext } from "./CompanionQuestion";
import { useCompanionNotices } from "./use-companion-notices";
import { CompanionNotifications } from "./CompanionNotifications";
import { CompanionTaskProgress } from "./CompanionTaskProgress";
import { useIslandMotion } from "./use-island-motion";
import { useCompanionSounds } from "./use-companion-sounds";
import { useVoice } from "@/hooks/use-voice";
import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import "./companion.css";

const compactGeometry: CompanionGeometry = companionGeometrySchema.parse({ expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 288, height: 32 });
const companionError = (cause: unknown, fallback: string) => typeof cause === "string" ? cause : libraryError(cause, fallback);
const roleLabel = (role: string) => ROLE_LABELS[role as keyof typeof ROLE_LABELS] ?? role;
const statusColor: Record<CompanionStatus, string> = {
  running: "text-primary", waiting: "text-onedark-yellow", reconnecting: "text-onedark-yellow",
  completed: "text-onedark-green", failed: "text-onedark-red", idle: "text-muted-foreground",
};
const usageSchema = z.array(accountUsageSchema.extend({ providerKind: z.enum(["openai-codex", "antigravity", "claude-code"]) }));
type CompanionUsage = z.infer<typeof usageSchema>[number];
const usageProviderLabels = { "openai-codex": "OpenAI Codex", antigravity: "Antigravity", "claude-code": "Claude Code" };

function Usage({ accounts, error, now }: { accounts: CompanionUsage[] | null; error: string | null; now: number }) {
  if (error && !accounts) return <p role="alert" className="text-xs text-onedark-yellow">{error}</p>;
  if (!accounts) return <div role="status" aria-label="Carregando limites" className="companion-usage-grid">{[0, 1].map(index => <div key={index} className="space-y-3 rounded-md border border-border p-3"><Skeleton className="h-4 w-28" /><Skeleton className="h-2" /><Skeleton className="h-4 w-24" /></div>)}</div>;
  return <div className="companion-usage-grid">
    {error && <p role="status" className="col-span-full text-xs text-onedark-yellow">{error}</p>}
    {!accounts.length && <p className="col-span-full py-8 text-center text-xs text-muted-foreground">Nenhum limite disponível nos provedores conectados.</p>}
    {accounts.map(account => <section key={account.alias} aria-label={`Limites de ${account.alias}`} className="min-w-0 space-y-2 rounded-md border border-border bg-sidebar/40 p-2.5">
      <div className="flex min-w-0 items-center gap-1.5 text-xs font-medium"><span role="img" aria-label={usageProviderLabels[account.providerKind]} className="flex shrink-0"><ProviderIcon kind={account.providerKind} /></span><Hint content={account.alias} whenTruncated><p className="min-w-0 flex-1 truncate">{aliasSuffix(account.alias)}</p></Hint>{account.fetchedAt && <time dateTime={new Date(account.fetchedAt).toISOString()} aria-label={`Atualizado às ${new Date(account.fetchedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}`} className="shrink-0 font-mono text-[9px] text-muted-foreground/70">{new Date(account.fetchedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</time>}</div>
      {account.error && <p role="status" className="text-[11px] text-onedark-yellow">{account.fetchedAt ? "Limites desatualizados" : "Limites indisponíveis"}</p>}
      {!account.error && !account.windows.length && <p className="text-[11px] text-muted-foreground">Nenhuma janela informada.</p>}
      <div className="companion-usage-windows">{account.windows.map(window => <div key={window.id} className="companion-usage-window min-w-0 space-y-1.5">
        <Hint content={window.group} whenTruncated><p className="truncate font-mono text-[10px] text-muted-foreground">{window.group}</p></Hint>
        <WindowBar window={window} now={now} stale={Boolean(error || account.error) || !account.fetchedAt || now - account.fetchedAt > 5 * 60_000} />
      </div>)}</div>
    </section>)}
  </div>;
}

export function Companion() {
  const voice = useVoice();
  const companionVoice = voice.active && voice.session?.owner === "companion";
  const calling = companionVoice && voice.session?.mode === "call";
  useEffect(() => {
    const preventNativeMenu = (event: MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", preventNativeMenu);
    return () => document.removeEventListener("contextmenu", preventNativeMenu);
  }, []);
  const [geometry, setGeometry] = useState<CompanionGeometry>(compactGeometry);
  const [snapshot, setSnapshot] = useState<CompanionSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [tab, setTab] = useState("activity");
  const [activityPicker, setActivityPicker] = useState(false);
  const [tasksKey, setTasksKey] = useState<string | null>(null);
  const [usage, setUsage] = useState<CompanionUsage[] | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [usageAt, setUsageAt] = useState(() => Date.now());
  const [usageAttempt, setUsageAttempt] = useState(0);
  const [usageBusy, setUsageBusy] = useState(false);
  const forceUsage = useRef(false);
  const [resizing, setResizing] = useState(false);
  const [closing, setClosing] = useState(false);
  const [peekingClosing, setPeekingClosing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [gesture, setGesture] = useState<RobotProps["gesture"]>("none");
  const gestureTimer = useRef<number | undefined>(undefined);
  const gestureFrame = useRef<number | undefined>(undefined);
  const taps = useRef<number[]>([]);
  const [gaze, setGaze] = useState({ x: 0, y: 0 });
  const [visible, setVisible] = useState(() => !document.hidden);
  const robotElement = useRef<HTMLButtonElement>(null);
  const robotPlacement = useRef<HTMLDivElement>(null);
  const islandElement = useRef<HTMLDivElement>(null);
  const chatPane = useRef<CompanionChatHandle>(null);
  const expansionVersion = useRef(0);
  const nativeExpanded = useRef(false);
  const collapseHandler = useRef<() => void>(() => {});
  const geometryRevision = useRef(0);
  const pointer = useRef<{ x: number; dragging: boolean } | null>(null);
  const moveInFlight = useRef(false);
  const dragOperation = useRef<Promise<unknown>>(Promise.resolve());
  const dragged = useRef(false);
  const [drafts] = useState(() => new Map<string, QuestionDraft>());
  const [chatQuestion, setChatQuestion] = useState<CompanionQuestionContext | null>(null);
  const { notice, sync: syncNotices, clear: clearNotices } = useCompanionNotices();
  const refreshSnapshot = useRef<() => void>(() => {});
  const acknowledgements = useRef(new Set<string>());
  const items = useMemo(() => snapshot?.items ?? [], [snapshot]);
  const waiting = items.filter(item => item.status === "waiting").length;
  const working = items.filter(item => item.status === "running" || item.status === "reconnecting").length;
  const attentive = items.filter(item => item.status !== "idle" && (!(item.status === "completed" || item.status === "failed") || !item.acknowledged));
  const notifications = useMemo(() => items.filter(item => !item.acknowledged && (item.status === "completed" || item.status === "failed")), [items]);
  const active = attentive.filter(item => item.status !== "completed" && item.status !== "failed");
  const current = attentive.find(item => item.status === "waiting") ?? attentive.find(item => item.status === "reconnecting")
    ?? attentive.find(item => item.status === "running") ?? attentive.reduce<CompanionItem | undefined>((latest, item) => !latest || item.updatedAt > latest.updatedAt ? item : latest, undefined);
  const focused = attentive.find(item => companionItemKey(item) === selectedKey) ?? current;
  const petCurrent = attentive.find(item => item.status === "waiting") ?? attentive.find(item => item.status === "reconnecting")
    ?? attentive.find(item => item.status === "running") ?? attentive.reduce<CompanionItem | undefined>((latest, item) => !latest || item.updatedAt > latest.updatedAt ? item : latest, undefined);
  const status: CompanionStatus = voice.active && ["thinking", "transcribing", "speaking", "synthesizing"].includes(voice.session?.phase ?? "") ? "running" : error && !snapshot ? "failed" : petCurrent?.status ?? "idle";
  const snapshotQuestion = focused?.status === "waiting" && (focused.pendingQuestion || focused.requiresConversation) ? focused : items.find(item => item.status === "waiting" && (item.pendingQuestion || item.requiresConversation));
  const question: CompanionQuestionContext | null = tab === "chat" ? chatQuestion : tab === "activity" && snapshotQuestion ? {
    conversationId: snapshotQuestion.conversationId, agentId: snapshotQuestion.agentId, title: snapshotQuestion.title,
    projectName: snapshotQuestion.projectName, request: snapshotQuestion.pendingQuestion, requiresConversation: snapshotQuestion.requiresConversation,
  } : null;
  const hasQuestion = Boolean(question);
  const restActivity = items.map(item => `${companionItemKey(item)}/${item.attentionId ?? item.status}`).sort().join("|");
  const rest = useRobotRest(working > 0 || waiting > 0 || hasQuestion || dragging || geometry.expanded || voice.active, restActivity);
  const bubbleWanted = Boolean(notice && !geometry.expanded);
  const bubbleVisible = (bubbleWanted || peekingClosing) && geometry.bubble && !geometry.expanded;
  const now = useRunningClock(geometry.expanded && items.some(item => (item.status === "running" || item.status === "reconnecting") && item.activeSince !== null));
  const sounds = useCompanionSounds(items, Boolean(snapshot), visible);
  const tasksOpen = Boolean(focused?.tasks.length && tasksKey === companionItemKey(focused) && !notifications.length);
  const detail = Boolean(question) || tab !== "activity" || activityPicker || tasksOpen;
  const notchInset = geometry.notchWidth > 0 ? geometry.headerHeight : 0;
  const requestedHeight = (detail ? 400 : notifications.length ? 180 : 160) + notchInset;
  const visualGeometry = useMemo(() => geometry.expanded && !closing && requestedHeight < geometry.surfaceHeight ? {
    ...geometry, surfaceHeight: requestedHeight,
    surfaceY: geometry.surfaceY + (geometry.robotVertical === "bottom" ? geometry.surfaceHeight - requestedHeight : 0),
  } : geometry, [geometry, closing, requestedHeight]);
  useIslandMotion(islandElement, robotPlacement, visualGeometry, geometry.expanded && !closing || geometry.bubble && !peekingClosing, geometry.expanded && detail, visible);
  useEffect(() => () => { window.clearTimeout(gestureTimer.current); if (gestureFrame.current !== undefined) window.cancelAnimationFrame(gestureFrame.current); }, []);

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
        if (alive && next.success) { geometryRevision.current++; nativeExpanded.current = next.data.expanded; setGeometry(next.data); }
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
      if (alive && revision === geometryRevision.current && next.success) { nativeExpanded.current = next.data.expanded; setGeometry(next.data); }
    }).catch(cause => { if (alive) setError(companionError(cause, "Não foi possível posicionar o assistente.")); });
    return () => { alive = false; void invoke("companion_set_interacting", { active: false }).catch(() => {}); };
  }, []);

  useEffect(() => {
    if (dragging || resizing || bubbleWanted === geometry.bubble) return;
    let alive = true;
    const revision = ++geometryRevision.current;
    const update = async () => {
      setPeekingClosing(!bubbleWanted && geometry.bubble);
      if (!bubbleWanted && geometry.bubble && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) await new Promise(resolve => window.setTimeout(resolve, 360));
      if (!alive) return;
      const next = companionGeometrySchema.safeParse(await invoke("set_companion_bubble", { visible: bubbleWanted }));
      if (alive && revision === geometryRevision.current && next.success) { setGeometry(next.data); setPeekingClosing(false); }
    };
    void update().catch(cause => { if (alive) setError(companionError(cause, "Não foi possível mostrar o aviso.")); });
    return () => { alive = false; };
  }, [bubbleWanted, geometry.bubble, dragging, resizing]);

  const acknowledge = useCallback(async (item: CompanionItem) => {
    const key = `${item.attentionId}/${item.revision ?? "legacy"}`;
    if (item.acknowledged || item.status !== "completed" && item.status !== "failed") return true;
    if (acknowledgements.current.has(key)) return false;
    acknowledgements.current.add(key);
    try {
      const result = companionSnapshotSchema.safeParse(await invoke("ack_companion_item", { conversationId: item.conversationId, agentId: item.agentId, attentionId: item.attentionId, revision: item.revision }));
      if (!result.success) throw new Error("Não foi possível confirmar que a atividade foi vista. Tente novamente.");
      syncNotices(result.data.items); setSnapshot(result.data);
      const current = result.data.items.find(current => current.conversationId === item.conversationId && current.agentId === item.agentId && current.attentionId === item.attentionId);
      if (current && !current.acknowledged) acknowledgements.current.delete(key);
      refreshSnapshot.current();
      return !current || current.acknowledged;
    } catch (cause) {
      acknowledgements.current.delete(key);
      setError(companionError(cause, "Não foi possível marcar a atividade como vista."));
      return false;
    }
  }, [syncNotices]);
  const viewedChat = useCallback((conversationId: string, revision: number) => {
    if (!visible || hasQuestion) return;
    const item = snapshot?.items.find(item => item.conversationId === conversationId && item.agentId === null);
    if (item?.revision !== undefined && item.revision <= revision) void acknowledge(item);
  }, [visible, hasQuestion, snapshot, acknowledge]);

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
    const request = ++expansionVersion.current;
    setResizing(true);
    geometryRevision.current++;
    try {
      if (!expanded && nativeExpanded.current && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        setClosing(true);
        sounds.play("close");
        await new Promise(resolve => window.setTimeout(resolve, 360));
      }
      if (request !== expansionVersion.current) return;
      if (expanded) { setClosing(false); setPeekingClosing(false); sounds.play("open"); }
      const revision = ++geometryRevision.current;
      const next = companionGeometrySchema.parse(await invoke("set_companion_expanded", { expanded, ...(expanded ? { height: requestedHeight } : {}) }));
      if (request === expansionVersion.current && revision === geometryRevision.current) { nativeExpanded.current = next.expanded; setGeometry(next); }
    }
    catch (cause) { setError(companionError(cause, "Não foi possível abrir o assistente.")); }
    finally { if (request === expansionVersion.current) { setResizing(false); setClosing(false); } }
  }, [requestedHeight, sounds]);
  useEffect(() => {
    collapseHandler.current = () => { void expand(false); };
  }, [expand]);
  useEffect(() => {
    if (!geometry.expanded || closing || resizing || geometry.surfaceHeight === requestedHeight) return;
    const revision = ++geometryRevision.current;
    let alive = true;
    const timer = window.setTimeout(() => {
      void invoke("set_companion_expanded", { expanded: true, height: requestedHeight }).then(value => {
        const next = companionGeometrySchema.safeParse(value);
        if (alive && revision === geometryRevision.current && next.success) setGeometry(next.data);
      }).catch(cause => { if (alive) setError(companionError(cause, "Não foi possível ajustar a ilha.")); });
    }, requestedHeight < geometry.surfaceHeight && !window.matchMedia("(prefers-reduced-motion: reduce)").matches ? 360 : 0);
    return () => { alive = false; window.clearTimeout(timer); };
  }, [geometry.expanded, geometry.surfaceHeight, requestedHeight, closing, resizing]);
  const toggleSound = () => { void sounds.toggle().catch(cause => setError(companionError(cause, "Não foi possível salvar os sons do Jarvito."))); };
  const openConversation = useCallback(async (target: { conversationId: string; agentId?: string | null }) => {
    const accessed = items.find(item => item.conversationId === target.conversationId && item.agentId === (target.agentId ?? null));
    try {
      if (accessed?.global) {
        if (!await chatPane.current?.openGeneral(accessed.conversationId, accessed.revision)) return;
        setTab("chat");
        if (!geometry.expanded) await expand(true);
      } else await invoke("companion_open_conversation", { conversationId: target.conversationId });
      if (accessed && !await acknowledge(accessed)) return;
      refreshSnapshot.current();
    }
    catch (cause) { setError(companionError(cause, "Não foi possível abrir a conversa.")); }
  }, [items, acknowledge, geometry.expanded, expand]);
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

  const poke = () => {
    if (gesture === "dizzy") return;
    const now = performance.now();
    taps.current = [...taps.current.filter(at => now - at < 800), now];
    const dizzy = taps.current.length >= 3;
    if (dizzy) taps.current = [];
    if (gestureFrame.current !== undefined) window.cancelAnimationFrame(gestureFrame.current);
    if (!dizzy && gesture === "poke") {
      setGesture("none"); gestureFrame.current = window.requestAnimationFrame(() => setGesture("poke"));
    } else setGesture(dizzy ? "dizzy" : "poke");
    sounds.play(dizzy ? "dizzy" : "poke");
    window.clearTimeout(gestureTimer.current);
    gestureTimer.current = window.setTimeout(() => setGesture("none"), dizzy ? 3000 : 500);
  };
  const move = () => {
    if (moveInFlight.current) return;
    moveInFlight.current = true;
    dragOperation.current = dragOperation.current.then(() => invoke("companion_move_horizontal")).catch(cause => setError(companionError(cause, "Não foi possível mover a ilha."))).finally(() => { moveInFlight.current = false; });
  };
  const finishDrag = () => {
    const moving = pointer.current?.dragging;
    pointer.current = null;
    if (moving) dragOperation.current = dragOperation.current.then(() => invoke("companion_finish_drag")).catch(cause => { setDragging(false); setError(companionError(cause, "Não foi possível posicionar a ilha.")); });
  };

  const pet = <div ref={robotPlacement} className="companion-robot-placement">
    <Button ref={robotElement} variant="ghost" className="companion-pet relative size-full cursor-pointer p-0 focus-visible:ring-2 focus-visible:ring-onedark-cyan/60" aria-label={geometry.expanded ? "Interagir com Jarvito" : `Abrir assistente Jarvis${waiting ? `, ${waiting} interações pendentes` : working ? `, ${working} atividades em andamento` : ""}`} aria-expanded={geometry.expanded && !closing} aria-controls="companion-panel"
      onPointerEnter={() => { rest.wake(); setHovered(true); sounds.play("hover"); }} onPointerLeave={() => setHovered(false)}
      onClick={() => { if (geometry.expanded) poke(); else { clearNotices(); void expand(true); } }}>
      <span className="companion-robot-facing">
        <Robot status={status} gesture={gesture !== "none" ? gesture : voice.active && voice.session?.phase === "listening" ? "listen" : voice.active && voice.session?.phase === "speaking" ? "speak" : rest.gesture} voiceLevel={voice.session?.level ?? 0} visible={visible} hovered={hovered} dragging={dragging} expanded={geometry.expanded} lookX={gaze.x} lookY={gaze.y} />
      </span>
    </Button>
  </div>;

  const panel = <Card id="companion-panel" role="region" aria-label="Assistente Jarvis" hidden={!geometry.expanded} className={`companion-panel min-h-0 gap-0 border-0 bg-transparent p-0 shadow-none ring-0 ${geometry.expanded ? "" : "hidden"}`} onPointerDownCapture={event => { if (question && event.target instanceof Element && event.target.closest("input, textarea, button, [contenteditable=true]")) void invoke("companion_set_interacting", { active: true }).catch(() => {}); }} onBlurCapture={event => { if (!(event.relatedTarget instanceof HTMLElement && event.currentTarget.contains(event.relatedTarget))) void invoke("companion_set_interacting", { active: false }).catch(() => {}); }}>
    <h1 className="sr-only">Jarvito</h1>
    <Tabs value={tab} onValueChange={value => { if (typeof value === "string") setTab(value); }} className="min-h-0 flex-1 gap-0">
      <header className="companion-header">
        {question ? <p className="ml-9 flex-1 truncate text-[11px] text-muted-foreground">{question.projectName} · Preciso de você</p> : <>
          <TabsList aria-label="Recursos do assistente" className="companion-navigation">
            <Hint content="Atividade"><TabsTrigger value="activity" aria-label="Atividade" className="companion-nav-button cursor-pointer"><Home className="size-3.5" /></TabsTrigger></Hint>
            <Hint content="Chat"><TabsTrigger value="chat" aria-label="Chat" className="companion-nav-button cursor-pointer"><MessageCircle className="size-3.5" /></TabsTrigger></Hint>
          </TabsList>
          <span className="flex-1" />
          <Hint content={calling ? "Encerrar ligação" : "Ligar para Jarvito"}><Button aria-label={calling ? "Encerrar ligação" : "Ligar para Jarvito"} variant="ghost" size="icon-xs" disabled={voice.active && !calling} className={`companion-nav-button cursor-pointer ${calling ? "text-onedark-green" : ""}`} onClick={() => { if (calling && voice.session?.id) void voice.control(voice.session.id, "end").catch(cause => setError(companionError(cause, "Não foi possível encerrar a ligação."))); else { setTab("chat"); chatPane.current?.startCall(); } }}>{calling ? <PhoneOff className="size-3.5" /> : <Phone className="size-3.5" />}</Button></Hint>
          <TabsList aria-label="Informações e ajustes" className="companion-navigation">
            <Hint content="Limites dos provedores"><TabsTrigger value="usage" aria-label="Limites" className="companion-nav-button cursor-pointer"><Gauge className="size-3.5" /></TabsTrigger></Hint>
            <Hint content="Ajustes"><TabsTrigger value="settings" aria-label="Ajustes" className="companion-nav-button cursor-pointer"><Settings2 className="size-3.5" /></TabsTrigger></Hint>
          </TabsList>
          <Hint content={sounds.enabled ? "Silenciar sons" : "Ativar sons"}><Button aria-label={sounds.enabled ? "Silenciar sons do Jarvito" : "Ativar sons do Jarvito"} disabled={!sounds.ready} variant="ghost" size="icon-xs" className="companion-nav-button cursor-pointer" onClick={toggleSound}>{sounds.enabled ? <Volume2 className="size-3.5" /> : <VolumeX className="size-3.5" />}</Button></Hint>
        </>}
      </header>
      {geometry.expanded && question && <CompanionQuestion context={question} drafts={drafts} onAnswer={answerQuestion} onInteract={pauseQuestion} onOpenConversation={() => { void openConversation(question); }} error={error} />}
      <div hidden={Boolean(question)} className={`companion-views min-h-0 flex-1 ${question ? "hidden" : ""}`}>
        <TabsContent value="activity" className="companion-overview" onPointerDownCapture={event => { if (event.target instanceof Element && event.target.closest("input, textarea, [contenteditable=true], [data-slot=select-trigger]")) void invoke("companion_set_interacting", { active: true }).catch(() => {}); }}>
          {notifications.length ? <CompanionNotifications items={notifications} error={error} onDismiss={acknowledge} onOpen={openConversation} /> : <Card data-state={focused?.status ?? "idle"} data-tasks-open={tasksOpen} className="companion-activity-card companion-focused-card" aria-label="Atividade selecionada">
            <div className="companion-activity-content">
              {error && <div className="space-y-1"><p role="alert" className="line-clamp-2 text-[11px] text-onedark-yellow">{error}</p><Button size="sm" variant="ghost" className="h-6 cursor-pointer px-0 text-[10px]" onClick={() => setAttempt(value => value + 1)}>Tentar novamente</Button></div>}
              {!snapshot && !error && <div role="status" aria-label="Carregando atividades" className="space-y-2 py-3"><Skeleton className="h-3 w-28" /><Skeleton className="h-3 w-40" /><Skeleton className="h-3 w-32" /></div>}
              {snapshot && !attentive.length && <div className="flex h-full flex-col justify-center gap-1"><h2 className="text-xs font-medium">Tudo tranquilo por aqui</h2><p className="text-[11px] leading-4 text-muted-foreground">Posso acompanhar seus projetos e conversar com você.</p><Button variant="ghost" size="sm" className="mt-1 h-6 w-fit cursor-pointer px-0 text-[10px] text-primary" onClick={() => { chatPane.current?.startGeneral(); setTab("chat"); }}>Vamos conversar<MessageCircle className="size-3" /></Button></div>}
              {focused && <>
                <div className="flex items-center gap-1.5"><span aria-hidden="true" className={`companion-state-dot ${statusColor[focused.status]}`} /><p className="min-w-0 flex-1 truncate text-[11px] font-medium">{focused.projectName}</p><span className="shrink-0 font-mono text-[9px] text-muted-foreground">{activeDuration(focused)}</span><Hint content={focused.global ? "Ver conversa com Jarvito" : "Abrir conversa no Jarvis"}><Button size="icon-xs" variant="ghost" aria-label={focused.global ? "Ver conversa com Jarvito" : "Abrir conversa no Jarvis"} className="companion-jump shrink-0 cursor-pointer" onClick={() => { void openConversation(focused); }}><ArrowUpRight className="size-3" /></Button></Hint></div>
                <div className="flex min-w-0 items-center gap-1">
                  <h2 className="min-w-0 flex-1 truncate text-[12px] font-semibold">{focused.title}</h2>
                  {attentive.length > 1 && <Select value={companionItemKey(focused)} onOpenChange={setActivityPicker} onValueChange={value => { if (typeof value === "string") setSelectedKey(value); }}><SelectTrigger aria-label="Conversa ou agente" size="sm" className="companion-activity-select cursor-pointer"><SelectValue><span className="sr-only">{focused.title}</span></SelectValue></SelectTrigger><SelectContent className="max-h-[calc(100vh-100px)] max-w-[calc(100vw-24px)]">{attentive.map(item => <SelectItem key={companionItemKey(item)} value={companionItemKey(item)} className="cursor-pointer py-2 text-[11px]"><span className="flex min-w-0 flex-col gap-1"><span className="truncate font-medium">{item.title}</span><span className="truncate font-mono text-[9px] text-muted-foreground">{item.projectName} · {roleLabel(item.role)}{item.agentId ? " · subagente" : ""} · {companionStatusLabels[item.status]}</span></span></SelectItem>)}</SelectContent></Select>}
                </div>
                <p key={`${companionItemKey(focused)}/${focused.status}`} className="companion-live-detail line-clamp-1 text-[11px] leading-4 text-muted-foreground">{focused.activity || `${roleLabel(focused.role)}${focused.agentId ? " · subagente" : ""}`}</p>
                <CompanionTaskProgress tasks={focused.tasks} active={focused.status === "running" || focused.status === "reconnecting"} open={tasksOpen} onOpenChange={open => setTasksKey(open ? companionItemKey(focused) : null)}><span className={`flex shrink-0 items-center gap-1 text-[10px] ${statusColor[focused.status]}`}>{focused.status === "reconnecting" && <Wifi className="size-3" />}{companionStatusLabels[focused.status]}</span></CompanionTaskProgress>
              </>}
            </div>
          </Card>}
          {(notifications.length ? active.length > 0 : active.length > 1) && <Card className="companion-activity-card companion-other-card" aria-label="Outras atividades"><div className="flex h-full min-w-0 flex-col gap-2 p-3"><p className="text-[10px] text-muted-foreground">Também estou acompanhando</p><ScrollArea className="min-h-0 flex-1">{active.filter(item => notifications.length || companionItemKey(item) !== (focused && companionItemKey(focused))).map(item => <Button key={companionItemKey(item)} variant="ghost" className="mb-1 h-auto w-full cursor-pointer justify-start gap-2 px-1 py-1 text-left" aria-label={notifications.length ? `Ver atividade: ${item.title}` : undefined} onClick={() => { if (notifications.length) void openConversation(item); else setSelectedKey(companionItemKey(item)); }}><span aria-hidden="true" className={`companion-state-dot ${statusColor[item.status]}`} /><span className="min-w-0 flex-1"><span className="block truncate text-[11px]">{item.projectName}</span><span className="block truncate text-[10px] text-muted-foreground">{item.title}</span>{notifications.length > 0 && <span className="mt-1 flex items-center gap-1 text-[10px] text-primary">Ver atividade<ArrowUpRight className="size-3" /></span>}</span></Button>)}</ScrollArea></div></Card>}
        </TabsContent>
        <TabsContent value="chat" keepMounted className="companion-detail-view min-h-0 overflow-hidden" onPointerDownCapture={event => { if (event.target instanceof Element && event.target.closest("input, textarea, [contenteditable=true], [data-slot=select-trigger], [aria-haspopup=menu]")) void invoke("companion_set_interacting", { active: true }).catch(() => {}); }}><CompanionChatPane ref={chatPane} active={geometry.expanded && tab === "chat" || companionVoice} externalQuestions onQuestionChange={setChatQuestion} onView={viewedChat} onSend={() => sounds.play("send")} /></TabsContent>
        <TabsContent value="usage" className="companion-detail-view min-h-0 overflow-hidden"><div className="mb-3 flex items-center justify-between gap-2"><h2 className="text-xs font-medium">Limites dos provedores</h2><Hint content="Atualizar limites"><Button aria-label="Atualizar limites" aria-busy={usageBusy} disabled={usageBusy} size="icon-xs" variant="ghost" className="cursor-pointer text-muted-foreground" onClick={() => { forceUsage.current = true; setUsageAttempt(value => value + 1); }}><RefreshCw className={`size-3 ${usageBusy ? "motion-safe:animate-spin" : ""}`} /></Button></Hint></div><ScrollArea className="h-[calc(100%-34px)] pr-2"><Usage accounts={usage} error={usageError} now={usageAt} /></ScrollArea></TabsContent>
        <TabsContent value="settings" className="companion-detail-view min-h-0"><h2 className="mb-3 text-xs font-medium">Ajustes do Jarvito</h2><Card className="companion-activity-card gap-0"><div className="flex items-center gap-4 p-4"><div className="min-w-0 flex-1"><label htmlFor="companion-sounds" className="cursor-pointer text-xs font-medium">Sons de interação</label><p className="mt-1 text-[11px] leading-4 text-muted-foreground">Abertura, perguntas, atividades e conclusões.</p></div><Switch id="companion-sounds" checked={sounds.enabled} disabled={!sounds.ready} onCheckedChange={toggleSound} className="cursor-pointer" /></div></Card><p className="mt-4 text-[11px] leading-5 text-muted-foreground">{geometry.dragAxis === "horizontal" ? "Arraste a área preta do cabeçalho para mover a ilha para os lados." : "A ilha fica junto à câmera, no topo da tela."} Ao usar outro aplicativo, ela recolhe sem interromper seu trabalho.</p></TabsContent>
      </div>
    </Tabs>
  </Card>;

  const speech = bubbleVisible && notice && <div className="companion-peek">
    <header className="companion-header"><span className="ml-2 flex-1 text-[10px] text-muted-foreground">Jarvito · {companionStatusLabels[notice.item.status]}</span><Hint content="Dispensar aviso"><Button variant="ghost" size="icon-xs" aria-label="Dispensar aviso" className="companion-nav-button cursor-pointer" onClick={clearNotices}><ChevronDown className="size-3.5" /></Button></Hint></header>
    <Card role="status" aria-label="Aviso do Jarvito" data-state={notice.item.status} className="companion-activity-card companion-speech gap-1">
      <p className="truncate text-[10px] text-muted-foreground">{notice.item.projectName} · {roleLabel(notice.item.role)}</p>
      <p className="line-clamp-1 break-words text-xs leading-4 font-medium">{notice.item.status === "waiting" ? notice.item.pendingQuestion?.questions[0]?.question ?? "Preciso de uma decisão sua para continuar." : notice.item.status === "completed" ? "Sua resposta está pronta." : "Não consegui concluir esta solicitação."}</p>
      {notice.item.status !== "waiting" && <div className="companion-speech-result line-clamp-2 text-[11px] leading-4 text-muted-foreground"><LazyChatMarkdown content={notice.item.status === "completed" ? notice.item.result || (notice.item.title === "Jarvito" ? "Abra o chat para ver a resposta." : notice.item.title) : notice.item.activity || "Abra o chat para ver o que aconteceu."} /></div>}
      <div className="mt-auto flex items-center gap-2"><Button size="sm" variant="secondary" className="companion-small-action cursor-pointer" onClick={() => { if (notice.item.status === "waiting") { setSelectedKey(companionItemKey(notice.item)); setTab("activity"); clearNotices(); void expand(true); } else void openConversation(notice.item); }}>{notice.item.status === "waiting" ? notice.item.pendingQuestion && !notice.item.requiresConversation ? "Responder pergunta" : "Ver solicitação" : "Ver atividade"}<ArrowUpRight className="size-3" /></Button>{notice.item.status !== "waiting" && <Button size="sm" variant="ghost" className="companion-small-action cursor-pointer" onClick={() => { void acknowledge(notice.item); }}>OK</Button>}</div>
    </Card>
  </div>;

  return <main className="companion dark h-full min-h-0 w-full text-foreground" style={{ "--companion-notch-inset": `${notchInset}px` } as CSSProperties} data-status={status} data-expanded={geometry.expanded} data-detail={detail} data-drag-axis={geometry.dragAxis} data-bubble={bubbleVisible} data-robot-side={geometry.robotSide} data-robot-vertical={geometry.robotVertical} data-closing={closing || peekingClosing} data-visible={visible} onPointerMove={event => {
    const rect = robotElement.current?.getBoundingClientRect();
    if (rect && event.buttons === 0) {
      const x = Math.round(Math.tanh((event.clientX - rect.x - rect.width / 2) / 100) * 10) / 10;
      const y = Math.round(Math.tanh((event.clientY - rect.y - rect.height / 2) / 100) * 10) / 10;
      setGaze(previous => previous.x === x && previous.y === y ? previous : { x, y });
      robotElement.current?.style.setProperty("--robot-look-x", `${x * 4}px`);
      robotElement.current?.style.setProperty("--robot-look-y", `${y * 3}px`);
    }
  }} onPointerDownCapture={rest.wake} onClickCapture={rest.wake} onKeyDownCapture={rest.wake} onWheelCapture={rest.wake} onPointerLeave={() => { setHovered(false); setGaze(previous => previous.x === 0 && previous.y === 0 ? previous : { x: 0, y: 0 }); robotElement.current?.style.setProperty("--robot-look-x", "0px"); robotElement.current?.style.setProperty("--robot-look-y", "0px"); }} onKeyDown={event => { if (event.key === "Escape" && geometry.expanded) { event.preventDefault(); void expand(false); } }}>
    <div ref={islandElement} className="companion-island" data-working={visible && working > 0 && !geometry.expanded && !bubbleVisible} onPointerDown={event => {
      const target = event.target;
      if (geometry.dragAxis !== "horizontal" || event.button !== 0 || !(target instanceof Element) || !target.closest(".companion-header, .companion-compact-open")) return;
      if (target.closest("button, input, textarea, [role=tab], [data-slot=select-trigger]") && !target.closest(".companion-compact-open")) return;
      event.currentTarget.setPointerCapture?.(event.pointerId);
      pointer.current = { x: event.screenX, dragging: false }; dragged.current = false;
    }} onPointerMove={event => {
      if (!(event.buttons & 1)) { finishDrag(); return; }
      const start = pointer.current;
      if (!start) return;
      if (!start.dragging) {
        if (Math.abs(event.screenX - start.x) < 5) return;
        start.dragging = true; dragged.current = true; setDragging(true); geometryRevision.current++;
        moveInFlight.current = true;
        dragOperation.current = dragOperation.current.then(() => invoke("companion_start_drag")).then(() => invoke("companion_move_horizontal")).catch(cause => { setDragging(false); setError(companionError(cause, "Não foi possível mover a ilha.")); }).finally(() => { moveInFlight.current = false; });
      } else move();
    }} onPointerUp={event => { finishDrag(); if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); }} onPointerCancel={finishDrag} onLostPointerCapture={finishDrag}>
      {panel}{speech}{pet}
      {!geometry.expanded && !bubbleVisible && <>
        <Button variant="ghost" className="companion-compact-open cursor-pointer" aria-label="Abrir ilha do Jarvito" aria-description={working > 0 ? "Jarvis está trabalhando" : undefined} aria-expanded="false" onClick={event => { if (dragged.current && event.detail !== 0) { dragged.current = false; return; } clearNotices(); void expand(true); }} />
        {(attentive.length > 0 || companionVoice) && <Badge variant="outline" data-working={visible && (working > 0 || companionVoice)} data-status={companionVoice ? "running" : status} className={`companion-compact-badge ${statusColor[companionVoice ? "running" : status]}`} aria-label={companionVoice ? "Microfone ativo no Jarvito" : `${attentive.length} atividades para acompanhar`} />}
      </>}
    </div>
  </main>;
}
