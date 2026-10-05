import { Fragment, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Activity, ArrowLeft, Check, ChevronDown, ChevronRight, Circle, CircleAlert, Folder, Layers3, ListFilter, LogOut, MessageSquare, RefreshCw, Smartphone, Wifi, WifiOff } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { Toaster } from "@/components/ui/sonner";
import { CardsSkeleton, DocumentSkeleton } from "@/components/layout/LoadingSkeletons";
import type { AgentTurn, DirectTask } from "@/core/chat";
import type { LibrarySelection } from "@/core/library";
import type { QuestionDraft } from "@/core/questions";
import { ROLE_LABELS, STATUS_LABELS } from "@/core/workflow";
import { RemoteClient, RemoteError, type RemoteBeads, type RemoteChat, type RemoteLibrary, type RemoteRuntime, type RemoteSession } from "./client";
import { PendingForms, type RemoteAction } from "./PendingForms";
import { aggregateActivity, currentActivity } from "./remote-activity";
import { RemoteQueuedMessages } from "./RemoteQueuedMessages";
import { RemoteComposer } from "./RemoteComposer";
import { RemoteUsage } from "./RemoteUsage";
import { RemoteChatSelectors } from "./RemoteChatSelectors";
import { remoteModelChoice, remoteModelProblem, remoteTurnOptions } from "./remote-choices";
import { chatAgentModelKey } from "@/core/chat-models";
import { flowSelection, type FlowSelection } from "@/core/workflow-catalog";
import type { ModelChoice } from "@/core/provider-references";
import type { RemoteChoices } from "./client";
import logoIcon from "../../public/logo_icon.png?url";

const emptySelection: LibrarySelection = { workspaceId: null, projectId: null, conversationId: null };
const attentionLabels: Record<RemoteRuntime["attention"][number]["kind"], string> = { question: "Pergunta", approval: "Permissão", publication: "Git e GitHub", authoring: "Configuração", validation: "Validação" };
const runtimeLabels: Record<NonNullable<RemoteRuntime["status"]>, string> = { running: "Executando", waiting: "Aguardando", idle: "Pronta", completed: "Concluída", failed: "Falhou", blocked: "Bloqueada", cancelled: "Cancelada", interrupted: "Interrompida" };

const taskLabels: Record<DirectTask["status"], string> = { pending: "Pendente", in_progress: "Em andamento", completed: "Concluída", blocked: "Bloqueada" };
const taskIcons = { pending: Circle, in_progress: Activity, completed: Check, blocked: CircleAlert };

function StatusBadge({ status, children }: { status: string; children: ReactNode }) {
  return <Badge variant="outline" className="remote-status" data-status={status}><span aria-hidden="true" className="remote-status-dot" />{children}</Badge>;
}

function ActivityCounts({ activity }: { activity: ReturnType<typeof aggregateActivity> }) {
  return <div className="remote-counts">
    {activity.running > 0 && <StatusBadge status="running">{activity.running} em execução</StatusBadge>}
    {activity.waiting > 0 && <StatusBadge status="waiting">{activity.waiting} precisa de você</StatusBadge>}
    {activity.failed > 0 && <StatusBadge status="failed">{activity.failed} com problema</StatusBadge>}
  </div>;
}

function NavigationCard({ title, subtitle, icon, status, onOpen, children }: { title: string; subtitle: string; icon: ReactNode; status: string; onOpen: () => void; children?: ReactNode }) {
  return <Card size="sm" className="remote-navigation-card" data-status={status}>
    <CardHeader className="remote-navigation-heading">
      <span className="remote-navigation-icon" aria-hidden="true">{icon}</span>
      <div className="min-w-0"><CardTitle>{title}</CardTitle><CardDescription>{subtitle}</CardDescription></div>
      <ChevronRight aria-hidden="true" className="remote-navigation-chevron" />
    </CardHeader>
    {children && <CardContent className="remote-navigation-meta">{children}</CardContent>}
    <Button variant="ghost" className="remote-navigation-open cursor-pointer" aria-label={title} onClick={onOpen}><span className="sr-only">{title}</span></Button>
  </Card>;
}

function BeadsPlan({ issues }: RemoteBeads) {
  const epics = issues.filter(issue => issue.issueType === "epic");
  if (!issues.length) return null;
  const taskRow = (issue: RemoteBeads["issues"][number]) => {
    const status: DirectTask["status"] = issue.status === "closed" ? "completed" : issue.status === "in_progress" ? "in_progress" : issue.status === "blocked" ? "blocked" : "pending";
    const Icon = taskIcons[status];
    return <li key={issue.id} className="remote-task" data-status={status}><Icon aria-hidden="true" /><div><p>{issue.title}</p><span>{issue.id} · {taskLabels[status]}</span></div></li>;
  };
  return <section className="flex flex-col gap-3" aria-label="Plano Beads"><p className="micro-label">Plano do projeto</p>{epics.map(epic => {
    const tasks = issues.filter(issue => issue.parentId === epic.id && issue.issueType !== "epic");
    const complete = tasks.filter(task => task.status === "closed").length;
    return <Card key={epic.id} size="sm" className="remote-epic"><CardHeader><CardDescription className="remote-epic-label"><Layers3 aria-hidden="true" />Épico <code>{epic.id}</code></CardDescription><CardTitle>{epic.title}</CardTitle></CardHeader>{tasks.length > 0 && <CardContent className="flex flex-col gap-3"><p className="text-xs text-muted-foreground">{complete} de {tasks.length} tarefas concluídas</p><Progress value={complete / tasks.length * 100} aria-label={`${complete} de ${tasks.length} tarefas do épico concluídas`} /><ul className="flex flex-col gap-2">{tasks.map(taskRow)}</ul></CardContent>}</Card>;
  })}{issues.some(issue => issue.issueType !== "epic" && !epics.some(epic => epic.id === issue.parentId)) && <ul className="flex flex-col gap-2">{issues.filter(issue => issue.issueType !== "epic" && !epics.some(epic => epic.id === issue.parentId)).map(taskRow)}</ul>}</section>;
}

function MobileInspector({ bundle, status, client }: { bundle: RemoteChat; status: string; client: RemoteClient }) {
  const [open, setOpen] = useState(false);
  const [beads, setBeads] = useState<RemoteBeads | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open) return;
    const controller = new AbortController(); let flight = false;
    const refresh = async () => {
      if (flight || document.visibilityState === "hidden") return;
      flight = true;
      try { const next = await client.beads(bundle.chat.conversationId, controller.signal); if (!controller.signal.aborted) { setBeads(next); setError(null); } }
      catch (cause) { if (!controller.signal.aborted) setError(cause instanceof Error ? cause.message : "Não foi possível carregar o plano do projeto."); }
      finally { flight = false; }
    };
    void refresh(); const timer = window.setInterval(() => { void refresh(); }, 15_000);
    return () => { controller.abort(); window.clearInterval(timer); };
  }, [open, client, bundle.chat.conversationId]);
  const active = bundle.chat.turns.find(turn => turn.id === bundle.chat.activeTurnId) ?? bundle.chat.turns[bundle.chat.turns.length - 1];
  const tasks = active?.tasks?.filter(task => !beads?.issues.some(issue => issue.issueType !== "epic" && (issue.id === task.id || issue.title === task.title))) ?? [];
  const completed = tasks.filter(task => task.status === "completed").length;
  const activity = currentActivity(bundle);
  return <Sheet open={open} onOpenChange={setOpen}><SheetTrigger render={<Button variant="ghost" size="icon" aria-label="Abrir Inspector" className="cursor-pointer" />}><ListFilter /></SheetTrigger>
    <SheetContent side="bottom" className="remote-sheet max-h-[85dvh]" showCloseButton={false}>
      <SheetHeader><SheetTitle>Inspector</SheetTitle><SheetDescription>Tarefas e andamento desta conversa.</SheetDescription></SheetHeader>
      <div className="flex min-h-0 flex-col gap-4 overflow-y-auto px-4 pb-2">
        <section className="flex flex-col gap-2"><StatusBadge status={activity.status}>{status}</StatusBadge>{activity.detail && <p className="break-words text-sm">{activity.detail}</p>}</section>
        {error && <Alert><AlertTitle>Plano indisponível</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>}
        {!beads && !error && <div role="status" aria-label="Carregando plano do projeto"><Skeleton className="h-14" /></div>}
        {beads && <BeadsPlan issues={beads.issues} />}
        {(tasks.length > 0 || !beads?.issues.some(issue => issue.issueType !== "epic")) && <><Separator /><section className="flex flex-col gap-3"><div className="flex items-center justify-between gap-2"><p className="micro-label">Tarefas</p>{tasks.length > 0 && <span className="font-mono text-xs text-muted-foreground">{completed}/{tasks.length}</span>}</div>
          {tasks.length > 0 ? <><Progress value={completed / tasks.length * 100} aria-label={`${completed} de ${tasks.length} tarefas concluídas`} /><ul className="flex flex-col gap-2">{tasks.map(task => {
            const Icon = taskIcons[task.status];
            return <li key={task.id} className="remote-task" data-status={task.status}><Icon aria-hidden="true" /><div><p>{task.title}</p><span>{taskLabels[task.status]}</span></div></li>;
          })}</ul></> : <p className="text-sm text-muted-foreground">Nenhuma tarefa registrada.</p>}
        </section></>}
        <Separator /><section className="flex flex-col gap-2"><p className="micro-label">Agentes</p>{bundle.workflow?.agents.length ? bundle.workflow.agents.map(agent => <Card key={agent.id} size="sm" className="remote-agent-card" data-status={agent.status}><CardHeader><CardTitle className="break-words">{agent.identity?.name ?? agent.title}</CardTitle><CardDescription>{ROLE_LABELS[agent.role]} · <span className="remote-agent-status">{STATUS_LABELS[agent.status]}</span></CardDescription></CardHeader>{agent.currentThought && <CardContent><p className="whitespace-pre-wrap break-words text-sm">{agent.currentThought}</p></CardContent>}</Card>) : <p className="text-sm text-muted-foreground">Jarvis · {bundle.chat.activeTurnId ? "Executando" : "Aguardando"}</p>}</section>
        <p className="break-all font-mono text-xs text-muted-foreground">{bundle.options?.model ?? active?.options.model ?? "Configuração do desktop"}</p>
      </div><SheetFooter><SheetClose render={<Button variant="outline" className="cursor-pointer" />}>Fechar Inspector</SheetClose></SheetFooter>
    </SheetContent>
  </Sheet>;
}

function MessageMarkdown({ content }: { content: string }) {
  return <div className="markdown-editor-prose remote-prose"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{
    a: ({ href, children }) => /^https?:\/\//i.test(href ?? "") ? <a href={href} target="_blank" rel="noreferrer" className="cursor-pointer">{children}</a> : <span>{children}</span>,
    img: ({ alt }) => <span className="text-muted-foreground">[Imagem: {alt || "sem descrição"}]</span>,
  }}>{content}</ReactMarkdown></div>;
}

function UserMessage({ content, additional = false }: { content: string; additional?: boolean }) {
  return <Card size="sm" className="remote-user-message"><CardHeader><CardDescription>{additional ? "Você · enviada durante a execução" : "Você"}</CardDescription></CardHeader><CardContent><p className="whitespace-pre-wrap break-words">{content}</p></CardContent></Card>;
}

function AuxiliaryMessages({ turn, boundary }: { turn: AgentTurn; boundary: number }) {
  return <>{turn.auxiliaryMessages?.filter(message => Math.min(message.afterStep ?? turn.steps.length, turn.steps.length) === boundary).map(message => <UserMessage key={message.id} content={message.content} additional />)}</>;
}

export function RemoteApp({ pairingToken = null, client: providedClient }: { pairingToken?: string | null; client?: RemoteClient }) {
  const [client] = useState(() => providedClient ?? new RemoteClient());
  const [session, setSession] = useState<RemoteSession | null>(null);
  const [auth, setAuth] = useState<"checking" | "pair" | "ready" | "expired" | "unavailable">(pairingToken !== null ? "pair" : "checking");
  const [name, setName] = useState("Meu celular");
  const [connection, setConnection] = useState<"connecting" | "connected" | "reconnecting" | "offline">("connecting");
  const [error, setError] = useState<string | null>(null);
  const [selection, setSelection] = useState<LibrarySelection>(emptySelection);
  const [library, setLibrary] = useState<RemoteLibrary | null>(null);
  const [bundle, setBundle] = useState<RemoteChat | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [questionDrafts] = useState(() => new Map<string, QuestionDraft>());
  const [formDrafts] = useState(() => new Map<string, string>());
  const [older, setOlder] = useState<Record<string, { turns: AgentTurn[]; start: number }>>({});
  const [busy, setBusy] = useState(false);
  const [readingDecision, setReadingDecision] = useState<string | null>(null);
  const [choices, setChoices] = useState<{ conversationId: string; data: RemoteChoices } | null>(null);
  const [choicesError, setChoicesError] = useState<{ conversationId: string; message: string } | null>(null);
  const [flows, setFlows] = useState<Record<string, FlowSelection>>({});
  const lock = useRef(false);
  const refreshRef = useRef<(force?: boolean) => Promise<void>>(async () => {});
  const refreshChoicesRef = useRef<(force?: boolean) => Promise<void>>(async () => {});
  const transcript = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);

  useEffect(() => {
    if (pairingToken !== null) return;
    const controller = new AbortController();
    let flight = false;
    let settled = false;
    const check = async () => {
      if (flight || settled || controller.signal.aborted || document.visibilityState === "hidden") return;
      flight = true;
      try {
        const value = await client.session(controller.signal);
        if (!controller.signal.aborted) { settled = true; setSession(value); setAuth("ready"); setError(null); }
      } catch (cause) {
        if (!controller.signal.aborted) {
          const revoked = cause instanceof RemoteError && cause.expired;
          settled = revoked;
          setAuth(revoked ? "expired" : "unavailable");
          setError(revoked ? "Este acesso expirou ou foi revogado. Leia um novo QR no computador." : "O computador está indisponível. Volte à rede dele para reconectar.");
        }
      } finally { flight = false; }
    };
    void check();
    const resume = () => { void check(); };
    const timer = window.setInterval(resume, 5_000);
    window.addEventListener("online", resume); window.addEventListener("focus", resume); document.addEventListener("visibilitychange", resume);
    return () => { controller.abort(); clearInterval(timer); window.removeEventListener("online", resume); window.removeEventListener("focus", resume); document.removeEventListener("visibilitychange", resume); };
  }, [client, pairingToken]);

  useEffect(() => {
    if (!session) return;
    let current = true;
    const flights = new Map<"library" | "chat", AbortController>();
    const read = async <T,>(source: "library" | "chat", request: (signal: AbortSignal) => Promise<T>, accept: (value: T) => void) => {
      if (flights.has(source)) return;
      const controller = new AbortController(); flights.set(source, controller);
      const relevant = () => current && !controller.signal.aborted;
      try {
        const value = await request(controller.signal);
        if (!relevant()) return;
        accept(value);
        if (source === "chat" || !selection.conversationId) { setConnection("connected"); setError(null); }
      } catch (cause) {
        if (!relevant()) return;
        if (cause instanceof RemoteError && cause.expired) { setSession(null); setAuth("expired"); setError("Este acesso expirou ou foi revogado. Leia um novo QR no computador."); }
        else if (source === "chat" || !selection.conversationId) { setConnection(navigator.onLine ? "reconnecting" : "offline"); setError(cause instanceof Error ? cause.message : "Não foi possível conectar ao computador."); }
      } finally { if (flights.get(source) === controller) flights.delete(source); }
    };
    const refresh = async (force = false) => {
      if (!current || document.visibilityState === "hidden") return;
      if (force) { flights.forEach(controller => controller.abort()); flights.clear(); }
      if (!navigator.onLine) { setConnection("offline"); return; }
      const libraryRead = read("library", signal => client.library(signal), setLibrary);
      if (selection.conversationId) {
        const conversationId = selection.conversationId;
        await read("chat", signal => client.chat(conversationId, signal), next => {
          setBundle(previous => previous?.chat.conversationId !== next.chat.conversationId ? next : {
            ...next,
            chat: previous.chat.revision > next.chat.revision ? previous.chat : next.chat,
            options: previous.chat.revision > next.chat.revision ? previous.options : next.options,
            workflow: previous.workflow && previous.workflow.revision > (next.workflow?.revision ?? -1) ? previous.workflow : next.workflow,
          });
        });
      } else await libraryRead;
    };
    refreshRef.current = refresh;
    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 2_000);
    const resume = () => { void refresh(); };
    const offline = () => { setConnection("offline"); };
    document.addEventListener("visibilitychange", resume); window.addEventListener("focus", resume); window.addEventListener("online", resume); window.addEventListener("offline", offline);
    return () => { current = false; flights.forEach(controller => controller.abort()); window.clearInterval(timer); document.removeEventListener("visibilitychange", resume); window.removeEventListener("focus", resume); window.removeEventListener("online", resume); window.removeEventListener("offline", offline); };
  }, [client, session, selection.conversationId]);

  useEffect(() => {
    const conversationId = selection.conversationId;
    if (!session || !conversationId) return;
    let current = true; let flight: AbortController | null = null;
    const refresh = async (force = false) => {
      if (!current || document.visibilityState === "hidden" || !navigator.onLine) return;
      if (flight && !force) return;
      flight?.abort();
      const controller = new AbortController(); flight = controller;
      try {
        const data = await client.choices(conversationId, controller.signal);
        if (current && !controller.signal.aborted) { setChoices({ conversationId, data }); setChoicesError(null); }
      } catch (cause) {
        if (current && !controller.signal.aborted) {
          if (cause instanceof RemoteError && cause.expired) { setSession(null); setAuth("expired"); setError("Este acesso expirou ou foi revogado. Leia um novo QR no computador."); }
          else setChoicesError({ conversationId, message: cause instanceof Error ? cause.message : "Não foi possível carregar os modelos e agentes." });
        }
      } finally { if (flight === controller) flight = null; }
    };
    refreshChoicesRef.current = refresh;
    void refresh();
    const resume = () => { void refresh(); };
    const timer = window.setInterval(resume, 15_000);
    window.addEventListener("focus", resume); window.addEventListener("online", resume); document.addEventListener("visibilitychange", resume);
    return () => { current = false; flight?.abort(); window.clearInterval(timer); window.removeEventListener("focus", resume); window.removeEventListener("online", resume); document.removeEventListener("visibilitychange", resume); };
  }, [client, session, selection.conversationId]);

  const loadUsage = useCallback(async (refresh = true) => {
    try { return await client.usage(refresh); }
    catch (cause) {
      if (cause instanceof RemoteError && cause.expired) { setSession(null); setAuth("expired"); setError("Este acesso expirou ou foi revogado. Leia um novo QR no computador."); }
      throw cause;
    }
  }, [client]);

  useEffect(() => {
    const viewport = window.visualViewport;
    let frame: number | null = null;
    const resize = () => {
      document.documentElement.style.setProperty("--remote-height", `${viewport?.height ?? window.innerHeight}px`);
      if (frame !== null) window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        frame = null;
        const focused = document.activeElement;
        if ((focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement) && focused.closest(".remote-scroll")) focused.scrollIntoView({ block: "nearest", inline: "nearest" });
      });
    };
    resize(); viewport?.addEventListener("resize", resize); window.addEventListener("resize", resize);
    return () => { if (frame !== null) window.cancelAnimationFrame(frame); viewport?.removeEventListener("resize", resize); window.removeEventListener("resize", resize); document.documentElement.style.removeProperty("--remote-height"); };
  }, []);

  const selectedBundle = bundle?.chat.conversationId === selection.conversationId ? bundle : null;
  const selectedProject = library?.library.projects.find(project => project.id === selection.projectId);
  const selectedWorkspace = library?.library.workspaces.find(workspace => workspace.id === selection.workspaceId);
  const selectedConversation = library?.library.conversations.find(conversation => conversation.id === selection.conversationId);
  const active = !!selectedBundle?.chat.activeTurnId || !!selectedBundle?.workflow?.agents.some(agent => ["queued", "running", "waiting"].includes(agent.status));
  const pendingDecision = !!(selectedBundle?.chat.pendingQuestion || selectedBundle?.chat.pendingApproval || selectedBundle?.chat.pendingAuthoring || selectedBundle?.workflow?.agents.some(agent => agent.pendingQuestion || agent.pendingApproval || agent.pendingAuthoring) || selectedBundle?.workflow?.validation && !selectedBundle.workflow.validation.submitted && !selectedBundle.workflow.validation.stale);
  const decisionKey = JSON.stringify([selection.conversationId, selectedBundle?.chat.pendingQuestion?.toolId, selectedBundle?.chat.pendingApproval?.tool.id, selectedBundle?.chat.pendingAuthoring?.toolId, (selectedBundle?.workflow?.agents ?? []).filter(agent => agent.pendingQuestion || agent.pendingApproval || agent.pendingAuthoring).map(agent => [agent.id, agent.pendingQuestion?.toolId, agent.pendingApproval?.tool.id, agent.pendingAuthoring?.toolId]), selectedBundle?.workflow?.validation?.id]);
  const focusedDecision = pendingDecision && readingDecision !== decisionKey;
  const activity = selectedBundle ? currentActivity(selectedBundle) : null;
  const conversationStatus = activity?.label ?? "Carregando conversa";
  const draft = drafts[selection.conversationId ?? ""] ?? "";
  const disabled = busy || connection !== "connected";
  const currentChoices = choices?.conversationId === selection.conversationId ? choices.data : null;
  const choiceFailure = choicesError?.conversationId === selection.conversationId ? choicesError.message : null;
  const previousOptions = selectedBundle?.options ?? selectedBundle?.chat.latestOptions ?? selectedBundle?.chat.turns[selectedBundle.chat.turns.length - 1]?.options ?? null;
  const selectedFlow = active ? flowSelection(previousOptions ?? undefined) : flows[selection.conversationId ?? ""] ?? flowSelection(previousOptions ?? undefined);
  const selectedModel = currentChoices ? remoteModelChoice(currentChoices, selectedFlow, previousOptions) : null;
  const modelProblem = choiceFailure ?? (currentChoices ? remoteModelProblem(currentChoices, selectedFlow, selectedModel) : null);

  useEffect(() => {
    const node = transcript.current;
    if (node && pinned.current && !focusedDecision) node.scrollTop = node.scrollHeight;
  }, [selectedBundle?.chat.revision, selectedBundle?.workflow?.revision, activity?.detail, focusedDecision]);
  useEffect(() => { pinned.current = true; }, [selection.conversationId]);

  const onAction: RemoteAction = async (method, params) => {
    if (lock.current || connection !== "connected") return false;
    lock.current = true; setBusy(true);
    try { await client.mutate(method, params); await refreshRef.current(true); toast.success(method === "message" ? "Mensagem enviada" : method === "queue_edit" ? "Mensagem atualizada" : method === "queue_delete" ? "Envio cancelado" : method === "queue_send_now" ? "Mensagem enviada agora" : "Decisão enviada"); return true; }
    catch (cause) {
      if (cause instanceof RemoteError && cause.expired) { setSession(null); setAuth("expired"); setError("Este acesso expirou ou foi revogado. Leia um novo QR no computador."); }
      else { toast.error(cause instanceof Error ? cause.message : "Não foi possível enviar a ação."); await refreshRef.current(true); }
      return false;
    } finally { lock.current = false; setBusy(false); }
  };

  const selectModel = async (choice: ModelChoice) => {
    const conversationId = selection.conversationId;
    if (!conversationId || lock.current || connection !== "connected") return false;
    lock.current = true; setBusy(true);
    try {
      const overrides = await client.setChatModel(conversationId, chatAgentModelKey(selectedFlow), choice);
      setChoices(current => current?.conversationId === conversationId ? { ...current, data: { ...current.data, overrides } } : current);
      await refreshChoicesRef.current(true);
      toast.success("Modelo atualizado neste chat");
      return true;
    } catch (cause) {
      if (cause instanceof RemoteError && cause.expired) { setSession(null); setAuth("expired"); setError("Este acesso expirou ou foi revogado. Leia um novo QR no computador."); }
      else { toast.error(cause instanceof Error ? cause.message : "Não foi possível atualizar o modelo deste chat."); await refreshChoicesRef.current(true); }
      return false;
    } finally { lock.current = false; setBusy(false); }
  };

  const pair = async () => {
    if (pairingToken === null || lock.current || !name.trim()) return;
    lock.current = true; setBusy(true); setError(null);
    try { const value = await client.pair(pairingToken, name.trim()); setSession(value); setAuth("ready"); toast.success("Celular conectado"); }
    catch (cause) { if (cause instanceof RemoteError && cause.code === "timeout") setAuth("expired"); setError(cause instanceof RemoteError && cause.code === "pairing_expired" ? "Este QR expirou ou já foi usado. Gere um novo QR no computador." : cause instanceof Error ? cause.message : "Não foi possível parear."); }
    finally { lock.current = false; setBusy(false); }
  };
  const reconnect = async () => {
    if (lock.current) return;
    lock.current = true; setBusy(true);
    try { const value = await client.session(); setSession(value); setAuth("ready"); setError(null); }
    catch (cause) { setError(cause instanceof RemoteError && cause.expired ? "Leia um novo QR no computador para autorizar este celular." : "O computador está indisponível. Verifique a rede e tente novamente."); }
    finally { lock.current = false; setBusy(false); }
  };
  const logout = async () => {
    if (lock.current) return;
    lock.current = true; setBusy(true);
    try { await client.logout(); setSession(null); setAuth("expired"); setLibrary(null); setBundle(null); setChoices(null); setChoicesError(null); setFlows({}); setDrafts({}); setOlder({}); setSelection(emptySelection); questionDrafts.clear(); formDrafts.clear(); setError("Celular desconectado. Leia um novo QR para entrar novamente."); toast.success("Celular desconectado"); }
    catch (cause) { toast.error(cause instanceof Error ? cause.message : "Não foi possível desconectar."); }
    finally { lock.current = false; setBusy(false); }
  };

  const selectConversation = (conversationId: string) => {
    const conversation = library?.library.conversations.find(item => item.id === conversationId);
    const project = library?.library.projects.find(item => item.id === conversation?.projectId);
    if (conversation && project) setSelection({ workspaceId: project.workspaceId, projectId: project.id, conversationId });
  };
  const back = () => setSelection(current => current.conversationId ? { ...current, conversationId: null } : current.projectId ? { ...current, projectId: null } : emptySelection);
  const needsYou = library?.runtime.filter(runtime => runtime.attention.length > 0) ?? [];
  const runtimeFor = (conversationId: string) => library?.runtime.find(runtime => runtime.conversationId === conversationId);
  const conversationIds = (projectIds: string[]) => library?.library.conversations.filter(item => projectIds.includes(item.projectId)).map(item => item.id) ?? [];
  const scopedIds = selection.projectId ? conversationIds([selection.projectId]) : selection.workspaceId ? conversationIds(library?.library.projects.filter(project => project.workspaceId === selection.workspaceId).map(project => project.id) ?? []) : library?.library.conversations.map(item => item.id) ?? [];
  const scopeActivity = library ? aggregateActivity(library, scopedIds) : null;
  const pendingRows = needsYou.map(runtime => {
    const conversation = library?.library.conversations.find(item => item.id === runtime.conversationId);
    const project = library?.library.projects.find(item => item.id === conversation?.projectId);
    return conversation && <Button key={runtime.conversationId} variant="outline" className="remote-pending-row h-auto w-full cursor-pointer justify-start whitespace-normal text-left" onClick={() => selectConversation(runtime.conversationId)}><CircleAlert data-icon="inline-start" /><span className="min-w-0 flex-1"><span className="block break-words">{conversation.title}</span><span className="block text-xs text-muted-foreground">{project?.name} · {Array.from(new Set(runtime.attention.map(item => attentionLabels[item.kind]))).join(", ")}</span></span><ChevronRight data-icon="inline-end" /></Button>;
  });

  if (auth !== "ready") return <main className="remote-shell flex items-center justify-center p-4"><Card className="w-full max-w-sm"><CardHeader><Smartphone className="size-6 text-primary" /><CardTitle>Jarvis no celular</CardTitle><CardDescription>{auth === "pair" ? "Autorize este celular para acompanhar e responder às suas conversas." : auth === "checking" ? "Verificando acesso…" : auth === "unavailable" ? "Reconectando ao computador…" : "Pareie novamente pelo Jarvis no computador."}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">
    {auth === "checking" ? <CardsSkeleton label="Verificando acesso" /> : auth === "pair" && <form id="pair-form" onSubmit={event => { event.preventDefault(); void pair(); }}><FieldGroup><Field><FieldLabel htmlFor="device-name">Nome do celular</FieldLabel><Input id="device-name" autoComplete="off" maxLength={80} value={name} disabled={busy} onChange={event => setName(event.target.value)} /></Field></FieldGroup></form>}
    {error && <Alert variant="destructive"><AlertTitle>{auth === "pair" ? "Pareamento indisponível" : "Acesso indisponível"}</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>}
  </CardContent><CardFooter>{auth === "pair" ? <Button form="pair-form" type="submit" className="w-full cursor-pointer" disabled={busy || !name.trim()}>{busy ? "Conectando…" : "Conectar celular"}</Button> : (auth === "expired" || auth === "unavailable") && <Button variant="outline" className="w-full cursor-pointer" disabled={busy} onClick={() => { void reconnect(); }}><RefreshCw data-icon="inline-start" />Tentar reconectar</Button>}</CardFooter></Card><Toaster position="top-center" /></main>;

  const history = older[selection.conversationId ?? ""];
  const turns = selectedBundle ? Array.from(new Map([...(history?.turns ?? []), ...selectedBundle.chat.turns].map(turn => [turn.id, turn])).values()).sort((a, b) => a.createdAt - b.createdAt) : [];
  return <main className="remote-shell flex flex-col">
    <header className="remote-header flex shrink-0 items-center gap-2 border-b border-border bg-sidebar px-3 py-2">
      {selection.workspaceId ? <Button variant="ghost" size="icon" className="cursor-pointer" aria-label="Voltar" onClick={back}><ArrowLeft /></Button> : <span className="remote-brand"><img src={logoIcon} alt="Jarvis" /></span>}
      <div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{selectedConversation?.title ?? selectedProject?.name ?? selectedWorkspace?.name ?? "Jarvis"}</p><p className="truncate text-xs text-muted-foreground">{selectedConversation ? `${selectedWorkspace?.name} / ${selectedProject?.name}` : selectedProject ? selectedWorkspace?.name : session?.name}</p></div>
      <Badge variant="outline" className="remote-connection" data-connected={connection === "connected"} aria-label={connection === "connected" ? "Conectado" : connection === "offline" ? "Sem rede" : "Reconectando"}>{connection === "connected" ? <Wifi aria-hidden="true" /> : <WifiOff aria-hidden="true" />}</Badge>
      <RemoteUsage load={loadUsage} />
      <Button variant="ghost" size="icon" className="cursor-pointer" aria-label="Desconectar celular" disabled={busy} onClick={() => { void logout(); }}><LogOut /></Button>
    </header>
    {error && <Alert variant="destructive" className="shrink-0 rounded-none"><AlertTitle>{connection === "offline" ? "Sem rede" : "Reconectando ao computador"}</AlertTitle><AlertDescription>{error}<Button variant="outline" className="mt-2 cursor-pointer" onClick={() => { void refreshRef.current(); }}><RefreshCw data-icon="inline-start" />Atualizar</Button></AlertDescription></Alert>}
    {selection.conversationId ? <>
      <div className="remote-chat-status flex shrink-0 items-center gap-2 border-b border-border px-3" data-status={activity?.status ?? "idle"}>
        <div role="status" className="min-w-0 flex-1"><StatusBadge status={activity?.status ?? "idle"}>{conversationStatus}</StatusBadge></div>
        {pendingDecision && <Button variant="ghost" size="sm" className="remote-decision-toggle cursor-pointer" onClick={() => setReadingDecision(focusedDecision ? decisionKey : null)}>{focusedDecision ? "Ver conversa" : "Responder"}</Button>}
        {needsYou.some(runtime => runtime.conversationId !== selection.conversationId) && <Sheet><SheetTrigger render={<Button variant="ghost" size="icon" aria-label={`Precisa de você ${needsYou.length}`} className="remote-attention-trigger cursor-pointer" />}><CircleAlert /><span className="remote-attention-count">{needsYou.length}</span></SheetTrigger><SheetContent side="bottom" className="remote-sheet max-h-[85dvh]" showCloseButton={false}><SheetHeader><SheetTitle>Precisa de você</SheetTitle><SheetDescription>Perguntas e decisões de todas as conversas.</SheetDescription></SheetHeader><div className="flex min-h-0 flex-col gap-2 overflow-y-auto px-4">{needsYou.map((runtime, index) => <SheetClose key={runtime.conversationId} render={pendingRows[index] || <Button /> } />)}</div><SheetFooter><SheetClose render={<Button variant="outline" className="cursor-pointer" />}>Fechar pendências</SheetClose></SheetFooter></SheetContent></Sheet>}
        {selectedBundle && <MobileInspector key={selectedBundle.chat.conversationId} client={client} bundle={selectedBundle} status={conversationStatus} />}
      </div>
      {selectedBundle && pendingDecision && <div hidden={!focusedDecision} className="remote-decision-pane remote-scroll" role="region" aria-label="Decisão pendente"><PendingForms focused bundle={selectedBundle} projectPath={selectedProject?.path ?? ""} busy={disabled} onAction={onAction} questionDrafts={questionDrafts} formDrafts={formDrafts} /></div>}
      <div ref={transcript} hidden={focusedDecision} className="remote-transcript remote-scroll flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3" aria-label="Conversa" onScroll={event => { const node = event.currentTarget; pinned.current = node.scrollHeight - node.scrollTop - node.clientHeight < 180; }}>
        {!selectedBundle ? <DocumentSkeleton label="Carregando conversa" /> : <>
          {(history?.start ?? selectedBundle.chat.history?.start ?? 0) > 0 && <Button variant="outline" className="cursor-pointer" disabled={disabled} onClick={() => {
            const conversationId = selection.conversationId;
            if (!conversationId || lock.current) return;
            lock.current = true; setBusy(true);
            void client.history(conversationId, history?.start ?? selectedBundle.chat.history?.start ?? 0).then(page => setOlder(current => ({ ...current, [conversationId]: { start: page.history.start, turns: [...page.turns, ...(current[conversationId]?.turns ?? [])] } }))).catch(cause => toast.error(cause instanceof Error ? cause.message : "Não foi possível carregar o histórico.")).finally(() => { lock.current = false; setBusy(false); });
          }}>Carregar mensagens anteriores</Button>}
          {turns.length === 0 && <Empty><EmptyHeader><EmptyTitle>Esta conversa está pronta</EmptyTitle><EmptyDescription>Escolha um fluxo ou agente e envie sua mensagem.</EmptyDescription></EmptyHeader></Empty>}
          {turns.map(turn => <article key={turn.id} className="flex min-w-0 flex-col gap-3"><UserMessage content={turn.user} />
            <section className="flex min-w-0 flex-col gap-3 px-1" aria-label="Resposta do Jarvis"><AuxiliaryMessages turn={turn} boundary={0} />{turn.steps.map((step, index) => <Fragment key={index}><div className="min-w-0 flex flex-col gap-2">
              {step.summary && <Collapsible className="remote-reasoning"><CollapsibleTrigger render={<Button variant="ghost" className="cursor-pointer justify-start whitespace-normal" />}><Activity data-icon="inline-start" />Raciocínio<ChevronDown data-icon="inline-end" /></CollapsibleTrigger><CollapsibleContent><p className="whitespace-pre-wrap break-words text-sm text-muted-foreground">{step.summary}</p></CollapsibleContent></Collapsible>}
              {step.text && <MessageMarkdown content={step.text} />}{step.tools.filter(tool => tool.status === "running" || tool.status === "error").map(tool => <StatusBadge key={tool.id} status={tool.status === "error" ? "failed" : "running"}>{tool.name} · {tool.status === "error" ? "Erro" : "Executando"}</StatusBadge>)}
            </div><AuxiliaryMessages turn={turn} boundary={index + 1} /></Fragment>)}{turn.error && <Alert variant="destructive"><AlertTitle>A execução falhou</AlertTitle><AlertDescription className="whitespace-pre-wrap break-words">{turn.error.message}</AlertDescription></Alert>}</section>
          </article>)}
          <RemoteQueuedMessages conversationId={selectedBundle.chat.conversationId} messages={selectedBundle.chat.queuedMessages ?? []} running={active} compacting={!!selectedBundle.chat.compacting} busy={disabled} onAction={onAction} />
        </>}
      </div>
      <footer hidden={focusedDecision} className="remote-footer shrink-0 border-t border-border bg-background px-3 pt-1"><RemoteComposer key={selection.conversationId} value={draft} working={active || !!selectedBundle?.chat.compacting} busy={disabled} sendDisabled={!currentChoices || !!modelProblem || !selectedModel} model={selectedModel?.model ?? "Escolher modelo"} activity={activity} selectors={currentChoices ? <>
        {active && <p className="micro-label px-2 text-muted-foreground">Próxima mensagem</p>}
        <RemoteChatSelectors catalog={currentChoices.catalog} modelGroups={currentChoices.models} flow={selectedFlow} choice={selectedModel} disabled={disabled || !!selectedBundle?.chat.compacting} flowDisabled={active} modelProblem={modelProblem} onFlowChange={flow => { const conversationId = selection.conversationId; if (conversationId && !active) setFlows(current => ({ ...current, [conversationId]: flow })); }} onModelChange={selectModel} />
      </> : <div role="status" aria-label="Carregando modelos e agentes"><Skeleton className="h-11 w-full" /></div>} selectionError={modelProblem} onChange={value => { const conversationId = selection.conversationId; if (conversationId) setDrafts(current => ({ ...current, [conversationId]: value })); }} onSubmit={() => {
        const conversationId = selection.conversationId; const content = draft.trim();
        if (conversationId && content && currentChoices && selectedModel && !modelProblem) void onAction("message", { conversationId, content, options: remoteTurnOptions(selectedFlow, selectedModel, previousOptions) }).then(accepted => { if (accepted) setDrafts(current => current[conversationId] === draft ? { ...current, [conversationId]: "" } : current); });
      }} onCancel={selectedBundle?.chat.activeTurnId ? () => { void onAction("cancel", { conversationId: selection.conversationId, turnId: selectedBundle.chat.activeTurnId }); } : undefined} /></footer>
    </> : <div className="remote-scroll remote-library flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
      {!library ? <CardsSkeleton label="Carregando biblioteca" /> : <>
        <div className="remote-library-heading"><p className="micro-label">{selection.projectId ? "Conversas" : selection.workspaceId ? "Projetos" : "Espaços"}</p><h1>{selection.projectId ? "Acompanhe as conversas" : selection.workspaceId ? "Seus projetos" : "Seu Jarvis por perto"}</h1>{scopeActivity && <ActivityCounts activity={scopeActivity} />}</div>
        {library.discoveringAttention && <p role="status" className="remote-discovery text-xs text-muted-foreground"><Activity aria-hidden="true" />Verificando pendências…</p>}
        {library.attentionDiscoveryFailed && <Alert><AlertTitle>Pendências incompletas</AlertTitle><AlertDescription>Algumas conversas não puderam ser verificadas. O Jarvis tentará novamente.</AlertDescription></Alert>}
        {needsYou.length > 0 && <Card size="sm" className="remote-attention-card"><CardHeader><CardTitle><CircleAlert aria-hidden="true" />Precisa de você <span className="font-mono">{needsYou.length}</span></CardTitle></CardHeader><CardContent className="flex flex-col gap-2">{pendingRows}</CardContent></Card>}
        <div className="remote-navigation-list" aria-label={selection.projectId ? "Conversas do projeto" : selection.workspaceId ? "Projetos do espaço" : "Espaços disponíveis"}>
        {selection.projectId ? library.library.conversations.filter(item => item.projectId === selection.projectId).sort((a, b) => (b.lastActivityAt ?? b.createdAt) - (a.lastActivityAt ?? a.createdAt)).map(conversation => {
          const runtime = runtimeFor(conversation.id);
          const activity = aggregateActivity(library, [conversation.id]);
          return <NavigationCard key={conversation.id} title={conversation.title} subtitle={runtime?.attention.length ? Array.from(new Set(runtime.attention.map(item => attentionLabels[item.kind]))).join(" · ") : selectedProject?.name ?? "Conversa"} icon={<MessageSquare />} status={activity.status} onOpen={() => selectConversation(conversation.id)}>
            <StatusBadge status={activity.status}>{runtime?.attention.length ? "Precisa de você" : runtime?.compacting ? "Compactando" : runtime?.status ? runtimeLabels[runtime.status] : runtime?.activeTurnId ? "Executando" : "Pronta"}</StatusBadge>
          </NavigationCard>;
        }) : selection.workspaceId ? library.library.projects.filter(item => item.workspaceId === selection.workspaceId).map(project => {
          const ids = conversationIds([project.id]); const activity = aggregateActivity(library, ids);
          return <NavigationCard key={project.id} title={project.name} subtitle={`${ids.length} conversa${ids.length === 1 ? "" : "s"}`} icon={<Folder />} status={activity.status} onOpen={() => setSelection(current => ({ ...current, projectId: project.id, conversationId: null }))}><ActivityCounts activity={activity} /></NavigationCard>;
        }) : library.library.workspaces.map(workspace => {
          const projects = library.library.projects.filter(project => project.workspaceId === workspace.id); const activity = aggregateActivity(library, conversationIds(projects.map(project => project.id)));
          return <NavigationCard key={workspace.id} title={workspace.name} subtitle={`${projects.length} projeto${projects.length === 1 ? "" : "s"} · ${activity.total} conversa${activity.total === 1 ? "" : "s"}`} icon={<Layers3 />} status={activity.status} onOpen={() => setSelection({ workspaceId: workspace.id, projectId: null, conversationId: null })}><ActivityCounts activity={activity} /></NavigationCard>;
        })}
        </div>
        {(selection.projectId ? !library.library.conversations.some(item => item.projectId === selection.projectId) : selection.workspaceId ? !library.library.projects.some(item => item.workspaceId === selection.workspaceId) : !library.library.workspaces.length) && <Empty><EmptyHeader><EmptyTitle>Nenhum item por aqui</EmptyTitle><EmptyDescription>Crie espaços, projetos e conversas no Jarvis do computador.</EmptyDescription></EmptyHeader></Empty>}
      </>}
    </div>}
    <Toaster position="top-center" />
  </main>;
}
