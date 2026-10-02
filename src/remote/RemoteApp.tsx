import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowUp, ChevronRight, CircleStop, Folder, ListFilter, LogOut, MessageSquare, RefreshCw, Smartphone, Wifi, WifiOff } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { Toaster } from "@/components/ui/sonner";
import { Textarea } from "@/components/ui/textarea";
import { CardsSkeleton, DocumentSkeleton } from "@/components/layout/LoadingSkeletons";
import type { AgentTurn } from "@/core/chat";
import type { LibrarySelection } from "@/core/library";
import type { QuestionDraft } from "@/core/questions";
import { ROLE_LABELS, STATUS_LABELS } from "@/core/workflow";
import { RemoteClient, RemoteError, type RemoteChat, type RemoteLibrary, type RemoteRuntime, type RemoteSession } from "./client";
import { PendingForms, type RemoteAction } from "./PendingForms";

const emptySelection: LibrarySelection = { workspaceId: null, projectId: null, conversationId: null };
const attentionLabels: Record<RemoteRuntime["attention"][number]["kind"], string> = { question: "Pergunta", approval: "Permissão", publication: "Git e GitHub", authoring: "Configuração", validation: "Validação" };
const runtimeLabels: Record<NonNullable<RemoteRuntime["status"]>, string> = { running: "Executando", waiting: "Aguardando", idle: "Pronta", completed: "Concluída", failed: "Falhou", blocked: "Bloqueada", cancelled: "Cancelada", interrupted: "Interrompida" };

function MobileInspector({ bundle, status }: { bundle: RemoteChat; status: string }) {
  const active = bundle.chat.turns.find(turn => turn.id === bundle.chat.activeTurnId) ?? bundle.chat.turns[bundle.chat.turns.length - 1];
  return <Sheet><SheetTrigger render={<Button variant="outline" aria-label="Abrir Inspector" className="cursor-pointer" />}><ListFilter data-icon="inline-start" />Inspector</SheetTrigger>
    <SheetContent side="bottom" className="remote-sheet max-h-[85dvh]" showCloseButton={false}>
      <SheetHeader><SheetTitle>Inspector</SheetTitle><SheetDescription>Status, agentes, tarefas e alterações desta conversa.</SheetDescription></SheetHeader>
      <div className="flex min-h-0 flex-col gap-4 overflow-y-auto px-4 pb-2">
        <section><p className="micro-label mb-2">Status</p><p>{status}</p><p className="break-all font-mono text-xs text-muted-foreground">{bundle.options?.model ?? active?.options.model ?? "Configuração do desktop"}</p></section>
        <Separator /><section className="flex flex-col gap-2"><p className="micro-label">Agentes</p>{bundle.workflow?.agents.length ? bundle.workflow.agents.map(agent => <Card key={agent.id} size="sm"><CardHeader><CardTitle className="break-words text-sm">{agent.identity?.name ?? agent.title}</CardTitle><CardDescription>{ROLE_LABELS[agent.role]} · {STATUS_LABELS[agent.status]}</CardDescription></CardHeader>{agent.currentThought && <CardContent><p className="whitespace-pre-wrap break-words text-sm">{agent.currentThought}</p></CardContent>}</Card>) : <p className="text-sm text-muted-foreground">Jarvis · {bundle.chat.activeTurnId ? "Executando" : "Aguardando"}</p>}</section>
        <Separator /><section><p className="micro-label mb-2">Tarefas</p>{active?.tasks?.length ? <ul className="flex flex-col gap-2">{active.tasks.map(task => <li key={task.id} className="flex items-start gap-2"><Badge variant="outline">{task.status === "completed" ? "Concluída" : task.status === "in_progress" ? "Em andamento" : task.status === "blocked" ? "Bloqueada" : "Pendente"}</Badge><span className="min-w-0 break-words">{task.title}</span></li>)}</ul> : <p className="text-sm text-muted-foreground">Nenhuma tarefa registrada.</p>}</section>
        <Separator /><section><p className="micro-label mb-2">Arquivos alterados</p>{bundle.chat.fileChanges?.length ? <ul className="flex flex-col gap-2">{bundle.chat.fileChanges.map(file => <li key={file.path} className="flex flex-wrap justify-between gap-2 rounded-md border border-border p-2"><code className="min-w-0 break-all font-mono text-xs">{file.path}</code><span className="font-mono text-xs"><span className="text-onedark-green">+{file.additions ?? "?"}</span> <span className="text-destructive">−{file.deletions ?? "?"}</span></span></li>)}</ul> : <p className="text-sm text-muted-foreground">Nenhuma alteração registrada.</p>}</section>
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
  const lock = useRef(false);
  const refreshRef = useRef<(force?: boolean) => Promise<void>>(async () => {});
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
    let flight: AbortController | null = null;
    const refresh = async (force = false) => {
      if (!current || document.visibilityState === "hidden") return;
      if (force) { flight?.abort(); flight = null; }
      if (flight) return;
      if (!navigator.onLine) { setConnection("offline"); return; }
      const controller = new AbortController(); flight = controller;
      try {
        const [nextLibrary, nextChat] = await Promise.all([client.library(controller.signal), selection.conversationId ? client.chat(selection.conversationId, controller.signal) : Promise.resolve(null)]);
        if (!current || controller.signal.aborted) return;
        setLibrary(nextLibrary);
        if (nextChat) setBundle(previous => previous?.chat.conversationId === nextChat.chat.conversationId && previous.chat.revision > nextChat.chat.revision ? previous : nextChat);
        setConnection("connected"); setError(null);
      } catch (cause) {
        if (!current || controller.signal.aborted) return;
        if (cause instanceof RemoteError && cause.expired) { setSession(null); setAuth("expired"); setError("Este acesso expirou ou foi revogado. Leia um novo QR no computador."); }
        else { setConnection(navigator.onLine ? "reconnecting" : "offline"); setError(cause instanceof Error ? cause.message : "Não foi possível conectar ao computador."); }
      } finally { if (flight === controller) flight = null; }
    };
    refreshRef.current = refresh;
    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 2_000);
    const resume = () => { void refresh(); };
    const offline = () => { setConnection("offline"); };
    document.addEventListener("visibilitychange", resume); window.addEventListener("focus", resume); window.addEventListener("online", resume); window.addEventListener("offline", offline);
    return () => { current = false; flight?.abort(); window.clearInterval(timer); document.removeEventListener("visibilitychange", resume); window.removeEventListener("focus", resume); window.removeEventListener("online", resume); window.removeEventListener("offline", offline); };
  }, [client, session, selection.conversationId]);

  useEffect(() => {
    const viewport = window.visualViewport;
    const resize = () => document.documentElement.style.setProperty("--remote-height", `${viewport?.height ?? window.innerHeight}px`);
    resize(); viewport?.addEventListener("resize", resize); window.addEventListener("resize", resize);
    return () => { viewport?.removeEventListener("resize", resize); window.removeEventListener("resize", resize); document.documentElement.style.removeProperty("--remote-height"); };
  }, []);

  const selectedBundle = bundle?.chat.conversationId === selection.conversationId ? bundle : null;
  const selectedProject = library?.library.projects.find(project => project.id === selection.projectId);
  const selectedWorkspace = library?.library.workspaces.find(workspace => workspace.id === selection.workspaceId);
  const selectedConversation = library?.library.conversations.find(conversation => conversation.id === selection.conversationId);
  const active = !!selectedBundle?.chat.activeTurnId || !!selectedBundle?.workflow?.agents.some(agent => ["queued", "running", "waiting"].includes(agent.status));
  const lastTurn = selectedBundle?.chat.turns[selectedBundle.chat.turns.length - 1];
  const pendingDecision = !!(selectedBundle?.chat.pendingQuestion || selectedBundle?.chat.pendingApproval || selectedBundle?.chat.pendingAuthoring || selectedBundle?.workflow?.agents.some(agent => agent.pendingQuestion || agent.pendingApproval || agent.pendingAuthoring) || selectedBundle?.workflow?.validation && !selectedBundle.workflow.validation.submitted && !selectedBundle.workflow.validation.stale);
  const conversationStatus = pendingDecision ? "Precisa de você" : selectedBundle?.chat.compacting ? "Compactando contexto" : active ? "Em execução" : lastTurn?.status === "completed" ? "Concluído" : lastTurn?.status === "error" ? "Falhou" : lastTurn ? "Interrompido" : "Aguardando";
  const draft = drafts[selection.conversationId ?? ""] ?? "";
  const disabled = busy || connection !== "connected";

  useEffect(() => {
    const node = transcript.current;
    if (node && pinned.current) node.scrollTop = node.scrollHeight;
  }, [selectedBundle?.chat.revision, selectedBundle?.workflow?.revision]);
  useEffect(() => { pinned.current = true; }, [selection.conversationId]);

  const onAction: RemoteAction = async (method, params) => {
    if (lock.current || connection !== "connected") return false;
    lock.current = true; setBusy(true);
    try { await client.mutate(method, params); await refreshRef.current(true); toast.success(method === "message" ? "Mensagem enviada" : "Decisão enviada"); return true; }
    catch (cause) {
      if (cause instanceof RemoteError && cause.expired) { setSession(null); setAuth("expired"); setError("Este acesso expirou ou foi revogado. Leia um novo QR no computador."); }
      else { toast.error(cause instanceof Error ? cause.message : "Não foi possível enviar a ação."); await refreshRef.current(true); }
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
    try { await client.logout(); setSession(null); setAuth("expired"); setLibrary(null); setBundle(null); setDrafts({}); setOlder({}); setSelection(emptySelection); questionDrafts.clear(); formDrafts.clear(); setError("Celular desconectado. Leia um novo QR para entrar novamente."); toast.success("Celular desconectado"); }
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
  const pendingRows = needsYou.map(runtime => {
    const conversation = library?.library.conversations.find(item => item.id === runtime.conversationId);
    const project = library?.library.projects.find(item => item.id === conversation?.projectId);
    return conversation && <Button key={runtime.conversationId} variant="outline" className="h-auto min-h-14 w-full cursor-pointer justify-start whitespace-normal text-left" onClick={() => selectConversation(runtime.conversationId)}><MessageSquare data-icon="inline-start" /><span className="min-w-0 flex-1"><span className="block break-words">{conversation.title}</span><span className="block text-xs text-muted-foreground">{project?.name} · {Array.from(new Set(runtime.attention.map(item => attentionLabels[item.kind]))).join(", ")}</span></span><ChevronRight data-icon="inline-end" /></Button>;
  });

  if (auth !== "ready") return <main className="remote-shell flex items-center justify-center p-4"><Card className="w-full max-w-sm"><CardHeader><Smartphone className="size-6 text-primary" /><CardTitle>Jarvis no celular</CardTitle><CardDescription>{auth === "pair" ? "Autorize este celular para acompanhar e responder às suas conversas." : auth === "checking" ? "Verificando acesso…" : auth === "unavailable" ? "Reconectando ao computador…" : "Pareie novamente pelo Jarvis no computador."}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">
    {auth === "checking" ? <CardsSkeleton label="Verificando acesso" /> : auth === "pair" && <form id="pair-form" onSubmit={event => { event.preventDefault(); void pair(); }}><FieldGroup><Field><FieldLabel htmlFor="device-name">Nome do celular</FieldLabel><Input id="device-name" autoComplete="off" maxLength={80} value={name} disabled={busy} onChange={event => setName(event.target.value)} /></Field></FieldGroup></form>}
    {error && <Alert variant="destructive"><AlertTitle>{auth === "pair" ? "Pareamento indisponível" : "Acesso indisponível"}</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>}
  </CardContent><CardFooter>{auth === "pair" ? <Button form="pair-form" type="submit" className="w-full cursor-pointer" disabled={busy || !name.trim()}>{busy ? "Conectando…" : "Conectar celular"}</Button> : (auth === "expired" || auth === "unavailable") && <Button variant="outline" className="w-full cursor-pointer" disabled={busy} onClick={() => { void reconnect(); }}><RefreshCw data-icon="inline-start" />Tentar reconectar</Button>}</CardFooter></Card><Toaster position="top-center" /></main>;

  const history = older[selection.conversationId ?? ""];
  const turns = selectedBundle ? Array.from(new Map([...(history?.turns ?? []), ...selectedBundle.chat.turns].map(turn => [turn.id, turn])).values()).sort((a, b) => a.createdAt - b.createdAt) : [];
  return <main className="remote-shell flex flex-col">
    <header className="flex shrink-0 items-center gap-2 border-b border-border bg-sidebar px-3 py-2">
      {selection.workspaceId ? <Button variant="ghost" size="icon" className="cursor-pointer" aria-label="Voltar" onClick={back}><ArrowLeft /></Button> : <Smartphone aria-hidden="true" className="size-5 shrink-0 text-primary" />}
      <div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{selectedConversation?.title ?? selectedProject?.name ?? selectedWorkspace?.name ?? "Jarvis"}</p><p className="truncate font-mono text-[10px] text-muted-foreground">{selectedConversation ? selectedProject?.name : selectedProject ? selectedWorkspace?.name : session?.name}</p></div>
      <Badge variant="outline" aria-label={connection === "connected" ? "Conectado" : connection === "offline" ? "Sem rede" : "Reconectando"}>{connection === "connected" ? <Wifi aria-hidden="true" /> : <WifiOff aria-hidden="true" />}</Badge>
      <Button variant="ghost" size="icon" className="cursor-pointer" aria-label="Desconectar celular" disabled={busy} onClick={() => { void logout(); }}><LogOut /></Button>
    </header>
    {selection.conversationId && needsYou.length > 0 && <Sheet><SheetTrigger render={<Button variant="secondary" className="min-h-11 w-full shrink-0 cursor-pointer rounded-none" />}>Precisa de você <Badge variant="outline" className="font-mono">{needsYou.length}</Badge></SheetTrigger><SheetContent side="bottom" className="remote-sheet max-h-[85dvh]" showCloseButton={false}><SheetHeader><SheetTitle>Precisa de você</SheetTitle><SheetDescription>Perguntas e decisões de todas as conversas.</SheetDescription></SheetHeader><div className="flex min-h-0 flex-col gap-2 overflow-y-auto px-4">{needsYou.map((runtime, index) => <SheetClose key={runtime.conversationId} render={pendingRows[index] || <Button /> } />)}</div><SheetFooter><SheetClose render={<Button variant="outline" className="cursor-pointer" />}>Fechar pendências</SheetClose></SheetFooter></SheetContent></Sheet>}
    {error && <Alert variant="destructive" className="shrink-0 rounded-none"><AlertTitle>{connection === "offline" ? "Sem rede" : "Reconectando ao computador"}</AlertTitle><AlertDescription>{error}<Button variant="outline" className="mt-2 cursor-pointer" onClick={() => { void refreshRef.current(); }}><RefreshCw data-icon="inline-start" />Atualizar</Button></AlertDescription></Alert>}
    {selection.conversationId ? <>
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-2"><Badge variant="secondary" role="status">{conversationStatus}</Badge>{selectedBundle && <MobileInspector bundle={selectedBundle} status={conversationStatus} />}</div>
      <div ref={transcript} className="remote-scroll flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3" aria-label="Conversa" onScroll={event => { const node = event.currentTarget; pinned.current = node.scrollHeight - node.scrollTop - node.clientHeight < 180; }}>
        {!selectedBundle ? <DocumentSkeleton label="Carregando conversa" /> : <>
          {(history?.start ?? selectedBundle.chat.history?.start ?? 0) > 0 && <Button variant="outline" className="cursor-pointer" disabled={disabled} onClick={() => {
            const conversationId = selection.conversationId;
            if (!conversationId || lock.current) return;
            lock.current = true; setBusy(true);
            void client.history(conversationId, history?.start ?? selectedBundle.chat.history?.start ?? 0).then(page => setOlder(current => ({ ...current, [conversationId]: { start: page.history.start, turns: [...page.turns, ...(current[conversationId]?.turns ?? [])] } }))).catch(cause => toast.error(cause instanceof Error ? cause.message : "Não foi possível carregar o histórico.")).finally(() => { lock.current = false; setBusy(false); });
          }}>Carregar mensagens anteriores</Button>}
          {turns.length === 0 && <Empty><EmptyHeader><EmptyTitle>Esta conversa está pronta</EmptyTitle><EmptyDescription>Envie uma mensagem para continuar com a configuração do desktop.</EmptyDescription></EmptyHeader></Empty>}
          {turns.map(turn => <article key={turn.id} className="flex min-w-0 flex-col gap-3"><Card size="sm"><CardHeader><CardDescription>Você</CardDescription></CardHeader><CardContent><p className="whitespace-pre-wrap break-words">{turn.user}</p></CardContent></Card>
            <section className="flex min-w-0 flex-col gap-3 px-1" aria-label="Resposta do Jarvis">{turn.steps.map((step, index) => <div key={index} className="min-w-0">{step.text && <MessageMarkdown content={step.text} />}{step.tools.filter(tool => tool.status === "running" || tool.status === "error").map(tool => <Badge key={tool.id} variant="outline" className="my-1 max-w-full whitespace-normal break-all font-mono text-[10px]">{tool.name} · {tool.status === "error" ? "Erro" : "Executando"}</Badge>)}</div>)}{turn.error && <Alert variant="destructive"><AlertTitle>A execução falhou</AlertTitle><AlertDescription className="whitespace-pre-wrap break-words">{turn.error.message}</AlertDescription></Alert>}</section>
          </article>)}
          {(selectedBundle.chat.queuedMessages?.length ?? 0) > 0 && <Alert><AlertTitle>Mensagens na fila</AlertTitle><AlertDescription>{selectedBundle.chat.queuedMessages?.map(message => <p key={message.id} className="whitespace-pre-wrap break-words">{message.content}</p>)}</AlertDescription></Alert>}
          <PendingForms bundle={selectedBundle} projectPath={selectedProject?.path ?? ""} busy={disabled} onAction={onAction} questionDrafts={questionDrafts} formDrafts={formDrafts} />
        </>}
      </div>
      <footer className="remote-footer shrink-0 border-t border-border bg-background px-3 pt-3"><form className="chat-composer relative flex flex-col gap-2 rounded-[22px] border border-border bg-card p-3" data-working={active} onSubmit={event => {
        event.preventDefault(); const conversationId = selection.conversationId; const content = draft.trim();
        if (conversationId && content) void onAction("message", { conversationId, content }).then(accepted => { if (accepted) setDrafts(current => current[conversationId] === draft ? { ...current, [conversationId]: "" } : current); });
      }}><FieldGroup><Field><FieldLabel className="sr-only" htmlFor="remote-message">Mensagem</FieldLabel><Textarea id="remote-message" className="max-h-36 min-h-16 resize-none" placeholder="Mensagem para o Jarvis…" maxLength={64000} value={draft} onChange={event => { const conversationId = selection.conversationId; if (conversationId) setDrafts(current => ({ ...current, [conversationId]: event.target.value })); }} /></Field></FieldGroup><div className="flex items-center justify-between gap-2"><p className="min-w-0 truncate font-mono text-[10px] text-muted-foreground">{selectedBundle?.options?.model ?? "Configuração do desktop"}</p>{selectedBundle?.chat.activeTurnId && <Button type="button" variant="destructive" size="icon" aria-label="Interromper execução" className="cursor-pointer" disabled={disabled} onClick={() => { void onAction("cancel", { conversationId: selection.conversationId, turnId: selectedBundle.chat.activeTurnId }); }}><CircleStop /></Button>}<Button type="submit" aria-label={active ? "Enviar mensagem para a fila" : "Enviar mensagem"} className="cursor-pointer" disabled={disabled || !draft.trim()}><ArrowUp data-icon="inline-start" />{active ? "Na fila" : "Enviar"}</Button></div></form></footer>
    </> : <div className="remote-scroll flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
      {!library ? <CardsSkeleton label="Carregando biblioteca" /> : <>
        {library.discoveringAttention && <p role="status" className="text-xs text-muted-foreground">Verificando pendências das conversas…</p>}
        {library.attentionDiscoveryFailed && <Alert><AlertTitle>Pendências incompletas</AlertTitle><AlertDescription>Algumas conversas não puderam ser verificadas. O Jarvis tentará novamente.</AlertDescription></Alert>}
        {needsYou.length > 0 && <Card><CardHeader><CardTitle>Precisa de você</CardTitle><CardDescription>Perguntas e decisões pendentes em todas as conversas.</CardDescription></CardHeader><CardContent className="flex flex-col gap-2">{pendingRows}</CardContent></Card>}
        <p className="micro-label">{selection.projectId ? "Conversas" : selection.workspaceId ? "Projetos" : "Workspaces"}</p>
        {selection.projectId ? library.library.conversations.filter(item => item.projectId === selection.projectId).map(conversation => <Button key={conversation.id} variant="outline" className="h-auto min-h-16 w-full cursor-pointer justify-start whitespace-normal text-left" onClick={() => selectConversation(conversation.id)}><MessageSquare data-icon="inline-start" /><span className="min-w-0 flex-1 break-words">{conversation.title}</span>{runtimeFor(conversation.id)?.attention.length ? <Badge variant="secondary">Precisa de você</Badge> : runtimeFor(conversation.id)?.status ? <Badge variant="outline">{runtimeLabels[runtimeFor(conversation.id)?.status ?? "idle"]}</Badge> : runtimeFor(conversation.id)?.activeTurnId && <Badge variant="outline">Executando</Badge>}<ChevronRight data-icon="inline-end" /></Button>) : selection.workspaceId ? library.library.projects.filter(item => item.workspaceId === selection.workspaceId).map(project => <Button key={project.id} variant="outline" className="h-auto min-h-16 w-full cursor-pointer justify-start whitespace-normal text-left" onClick={() => setSelection(current => ({ ...current, projectId: project.id, conversationId: null }))}><Folder data-icon="inline-start" /><span className="min-w-0 flex-1"><span className="block break-words">{project.name}</span><span className="block break-all font-mono text-[10px] text-muted-foreground">{project.path}</span></span><ChevronRight data-icon="inline-end" /></Button>) : library.library.workspaces.map(workspace => <Button key={workspace.id} variant="outline" className="h-auto min-h-16 w-full cursor-pointer justify-start whitespace-normal text-left" onClick={() => setSelection({ workspaceId: workspace.id, projectId: null, conversationId: null })}><Folder data-icon="inline-start" /><span className="min-w-0 flex-1 break-words">{workspace.name}</span><ChevronRight data-icon="inline-end" /></Button>)}
        {(selection.projectId ? !library.library.conversations.some(item => item.projectId === selection.projectId) : selection.workspaceId ? !library.library.projects.some(item => item.workspaceId === selection.workspaceId) : !library.library.workspaces.length) && <Empty><EmptyHeader><EmptyTitle>Nenhum item por aqui</EmptyTitle><EmptyDescription>Crie workspaces, projetos e conversas no Jarvis do computador.</EmptyDescription></EmptyHeader></Empty>}
      </>}
    </div>}
    <Toaster position="top-center" />
  </main>;
}
