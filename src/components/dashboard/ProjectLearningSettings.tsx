import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BookOpen, Brain, ChevronDown, Download, Pencil, RefreshCw, Trash2, Upload } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Hint } from "@/components/ui/hint";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { MarkdownEditor } from "@/components/MarkdownEditor";
import { libraryError } from "@/core/library";
import { LESSON_STATUS, learningSnapshotSchema, lessonEdit, type LearningSnapshot, type ProjectLesson } from "@/core/project-learning";
import { knowledgeSnapshotSchema, type KnowledgeDocument } from "@/core/project-knowledge";

export function ProjectLearningSettings({ projectId }: { projectId: string }) {
  const [snapshot, setSnapshot] = useState<LearningSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("");
  const [draft, setDraft] = useState<ProjectLesson | null>(null);
  const [deletion, setDeletion] = useState<ProjectLesson | null>(null);
  const [importText, setImportText] = useState<string | null>(null);
  const [promotion, setPromotion] = useState<{ document: KnowledgeDocument; content: string } | null>(null);
  const mounted = useRef(true);
  const request = useRef(0);
  const reload = useCallback(() => {
    const id = ++request.current;
    return invoke("get_project_learning", { projectId }).then(value => {
      const next = learningSnapshotSchema.parse(value);
      if (mounted.current && id === request.current) { setSnapshot(next); setError(null); }
    }).catch(cause => {
      if (mounted.current && id === request.current) setError(libraryError(cause, "Não foi possível carregar os aprendizados."));
    });
  }, [projectId]);
  useEffect(() => {
    mounted.current = true;
    void reload();
    const events = listen<string>("project:learning-changed", ({ payload }) => { if (payload === projectId) void reload(); }).catch(() => () => {});
    return () => { mounted.current = false; request.current += 1; void events.then(unlisten => unlisten()); };
  }, [projectId, reload]);

  const mutate = async (command: string, args: Record<string, unknown>, message: string) => {
    setBusy(true); setError(null);
    try {
      const next = learningSnapshotSchema.parse(await invoke(command, { projectId, ...args }));
      if (mounted.current) { request.current += 1; setSnapshot(next); setDraft(null); setDeletion(null); setImportText(null); toast.success(message); }
    } catch (cause) { if (mounted.current) setError(libraryError(cause, "Não foi possível salvar. Seu rascunho foi preservado.")); }
    finally { if (mounted.current) setBusy(false); }
  };
  const exportLessons = async () => {
    setBusy(true);
    try { if (await invoke<boolean>("export_project_learning", { projectId })) toast.success("Aprendizados exportados sem o histórico das conversas"); }
    catch (cause) { if (mounted.current) setError(libraryError(cause, "Não foi possível exportar.")); }
    finally { if (mounted.current) setBusy(false); }
  };
  const previewImport = async () => {
    setBusy(true);
    try { const data = await invoke<unknown>("preview_project_learning_import"); if (data !== null && mounted.current) setImportText(JSON.stringify(data, null, 2)); }
    catch (cause) { if (mounted.current) setError(libraryError(cause, "Não foi possível abrir o arquivo.")); }
    finally { if (mounted.current) setBusy(false); }
  };
  const promote = async (lesson: ProjectLesson) => {
    setBusy(true); setError(null);
    try {
      const knowledge = knowledgeSnapshotSchema.parse(await invoke("get_project_knowledge", { projectId, scope: lesson.scope }));
      const document = knowledge.documents.find(d => d.kind === "rules" && d.scope === lesson.scope);
      if (!document || document.error) throw new Error(document?.error ?? "Documento de regras indisponível.");
      if (mounted.current) setPromotion({ document, content: `${document.content.trimEnd()}\n\n- ${lesson.content}${lesson.check ? ` Verificação: ${lesson.check}` : ""}\n`.trimStart() });
    } catch (cause) { if (mounted.current) setError(libraryError(cause, "Não foi possível preparar a regra.")); }
    finally { if (mounted.current) setBusy(false); }
  };
  const savePromotion = async () => {
    if (!promotion) return;
    setBusy(true);
    try {
      const { document, content } = promotion;
      await invoke("save_project_knowledge", { projectId, document: { kind: document.kind, scope: document.scope, content, essential: document.essential, revision: document.revision, sources: document.sources } });
      if (mounted.current) { setPromotion(null); toast.success("Aprendizado incorporado às regras do projeto"); }
    } catch (cause) { if (mounted.current) setError(libraryError(cause, "Não foi possível salvar as regras. Seu rascunho foi preservado.")); }
    finally { if (mounted.current) setBusy(false); }
  };

  const lessons = snapshot?.lessons.filter(l => `${l.content} ${l.scope} ${l.topics.join(" ")}`.toLocaleLowerCase().includes(filter.toLocaleLowerCase())) ?? [];
  const conflict = draft && snapshot?.lessons.find(l => l.id === draft.id)?.revision !== draft.revision;
  return <Card className="min-w-0">
    <CardHeader className="border-b border-border">
      <div className="flex items-center justify-between gap-3">
        <CardTitle className="flex items-center gap-2"><Brain aria-hidden="true" className="size-4 text-onedark-purple" />Aprendizados do projeto</CardTitle>
        <Hint content="Recarregar aprendizados"><Button variant="ghost" size="icon-sm" aria-label="Recarregar aprendizados" disabled={busy} onClick={() => void reload()}><RefreshCw className="size-4" /></Button></Hint>
      </div>
      <CardDescription>O Jarvis guarda correções úteis e consulta as lições relevantes em outros chats deste projeto. Elas complementam suas regras e podem ser editadas ou excluídas.</CardDescription>
    </CardHeader>
    <CardContent className="min-w-0 space-y-4 pt-5">
      {error && !draft && !deletion && importText === null && !promotion && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
      {!snapshot && !error && <div role="status" aria-label="Carregando aprendizados"><Skeleton className="mb-3 h-10 w-full" /><Skeleton className="h-24 w-full" /></div>}
      {snapshot && <>
        <div className="flex items-center justify-between gap-6">
          <div className="space-y-1"><Label htmlFor="project-learning-enabled">Aprender com minhas correções</Label><p className="max-w-2xl text-xs leading-relaxed text-muted-foreground">Analisa feedback novo em segundo plano usando o modelo do chat. Pode consumir tokens adicionais. Desativar pausa o registro e o uso das lições, preservando os dados.</p></div>
          <Switch id="project-learning-enabled" checked={snapshot.enabled} disabled={busy} onCheckedChange={enabled => void mutate("set_project_learning", { enabled, revision: snapshot.revision }, enabled ? "Aprendizado ativado" : "Aprendizado desativado")} />
        </div>
        {snapshot.pending > 0 && <p role="status" className="text-xs text-muted-foreground">{snapshot.pending} feedback(s) aguardando análise em segundo plano. O chat continua normalmente.</p>}
        {snapshot.notice && <Alert><AlertDescription>{snapshot.notice}</AlertDescription></Alert>}
        <div className="flex flex-wrap items-center gap-2">
          <Input aria-label="Buscar aprendizados" placeholder="Buscar por assunto ou pasta…" value={filter} onChange={e => setFilter(e.target.value)} className="min-w-0 flex-1 sm:max-w-sm" />
          <Button variant="outline" size="sm" disabled={busy || snapshot.lessons.length === 0} onClick={() => void exportLessons()}><Download className="size-3.5" />Exportar</Button>
          <Button variant="outline" size="sm" disabled={busy} onClick={() => void previewImport()}><Upload className="size-3.5" />Importar</Button>
          <span className="text-xs text-muted-foreground">{snapshot.lessons.length}/200 lições</span>
        </div>
        {lessons.length === 0 && <p className="rounded-md border border-dashed p-5 text-sm leading-relaxed text-muted-foreground">{snapshot.lessons.length ? "Nenhum aprendizado corresponde à busca." : "As correções recorrentes e orientações reutilizáveis aparecerão aqui. Pedidos pontuais não viram regras permanentes."}</p>}
        <div className="space-y-2">{lessons.map(lesson => <Collapsible key={lesson.id} className="min-w-0 rounded-md border bg-muted/15">
          <CollapsibleTrigger className="flex w-full cursor-pointer items-start gap-3 p-3 text-left">
            <ChevronDown aria-hidden="true" className="mt-1 size-3.5 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 space-y-2"><span className="block text-sm leading-relaxed">{lesson.content}</span><span className="flex flex-wrap items-center gap-2"><Badge variant={lesson.status === "active" ? "secondary" : "outline"}>{LESSON_STATUS[lesson.status]}</Badge><span className="font-mono text-[11px] text-muted-foreground">{lesson.scope === "." ? "Projeto inteiro" : lesson.scope}</span></span></span>
          </CollapsibleTrigger>
          <CollapsibleContent className="space-y-3 border-t p-3">
            {lesson.check && <p className="text-xs leading-relaxed"><span className="text-muted-foreground">Como verificar: </span>{lesson.check}</p>}
            <p className="text-[11px] text-muted-foreground">{lesson.origin === "user" ? "Editado por você" : lesson.origin === "imported" ? "Importado · revise antes de ativar" : "Aprendido com seu feedback"} · {new Date(lesson.updatedAt).toLocaleDateString("pt-BR")}</p>
            {lesson.evidence.map(e => <blockquote key={`${e.conversationId}:${e.messageId}`} className="break-words border-l-2 pl-3 text-xs leading-relaxed text-muted-foreground"><p>{e.excerpt}</p><p className="mt-1 font-mono text-[10px]">Conversa {e.conversationId} · {new Date(e.createdAt).toLocaleDateString("pt-BR")}</p></blockquote>)}
            <div className="flex flex-wrap gap-2">
              <Button size="sm" variant="outline" disabled={busy} onClick={() => { setError(null); setDraft({ ...lesson }); }}><Pencil className="size-3.5" />Editar</Button>
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => void promote(lesson)}><BookOpen className="size-3.5" />Incorporar às regras</Button>
              <Button size="sm" variant="ghost" className="text-destructive" disabled={busy} onClick={() => setDeletion(lesson)}><Trash2 className="size-3.5" />Excluir</Button>
            </div>
          </CollapsibleContent>
        </Collapsible>)}</div>
        <p className="text-[11px] leading-relaxed text-muted-foreground">Guardado nesta instalação. Use Exportar para levar as lições a outro projeto ou computador; trechos das conversas não são exportados. O backup geral de configurações não inclui estes dados.</p>
      </>}
    </CardContent>
    <Dialog open={draft !== null} onOpenChange={open => { if (!open && !busy) setDraft(null); }}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
      <DialogHeader><DialogTitle>Editar aprendizado</DialogTitle><DialogDescription>A lição deve ser curta e indicar quando se aplica. Sugestões só são usadas depois de ativadas.</DialogDescription></DialogHeader>
      {draft && <div className="min-w-0 space-y-4">
        {(error || conflict) && <Alert variant="destructive"><AlertDescription>{error ?? "Esta lição mudou. Feche e abra a edição novamente para usar a versão atual; seu rascunho foi preservado."}</AlertDescription></Alert>}
        <div className="space-y-2"><Label htmlFor="lesson-content">Lição</Label><Textarea id="lesson-content" value={draft.content} maxLength={600} rows={4} onChange={e => setDraft({ ...draft, content: e.target.value })} /></div>
        <div className="grid min-w-0 gap-4 sm:grid-cols-2">
          <div className="space-y-2"><Label htmlFor="lesson-scope">Pasta do projeto</Label><Input id="lesson-scope" value={draft.scope} onChange={e => setDraft({ ...draft, scope: e.target.value })} /><p className="text-[11px] text-muted-foreground">Use . para o projeto inteiro ou uma pasta relativa.</p></div>
          <div className="min-w-0 space-y-2"><Label htmlFor="lesson-status">Uso pelos agentes</Label><Select value={draft.status} items={Object.entries(LESSON_STATUS).map(([value,label]) => ({ value,label }))} onValueChange={value => { if (value && value in LESSON_STATUS) setDraft({ ...draft, status: value as ProjectLesson["status"] }); }}><SelectTrigger id="lesson-status" className="w-full"><SelectValue /></SelectTrigger><SelectContent>{Object.entries(LESSON_STATUS).map(([value,label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectContent></Select></div>
        </div>
        <div className="space-y-2"><Label htmlFor="lesson-topics">Assuntos, separados por vírgula</Label><Input id="lesson-topics" value={draft.topics.join(", ")} onChange={e => setDraft({ ...draft, topics: e.target.value.split(",").map(v => v.trim()) })} /></div>
        <div className="space-y-2"><Label htmlFor="lesson-check">Orientação de verificação</Label><Textarea id="lesson-check" value={draft.check} maxLength={300} rows={2} onChange={e => setDraft({ ...draft, check: e.target.value })} /></div>
      </div>}
      <DialogFooter><Button variant="outline" disabled={busy} onClick={() => setDraft(null)}>Cancelar</Button><Button disabled={busy || !!conflict || !draft?.content.trim()} onClick={() => draft && void mutate("save_project_lesson", { lesson: { ...lessonEdit(draft), topics: draft.topics.filter(Boolean) } }, "Aprendizado atualizado")}>Salvar aprendizado</Button></DialogFooter>
    </DialogContent></Dialog>
    <Dialog open={deletion !== null} onOpenChange={open => { if (!open && !busy) setDeletion(null); }}><DialogContent><DialogHeader><DialogTitle>Excluir aprendizado?</DialogTitle><DialogDescription>O Jarvis deixará de usar esta lição. O feedback antigo não a recriará automaticamente. A conversa original será preservada.</DialogDescription></DialogHeader>{error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}<DialogFooter><Button variant="outline" disabled={busy} onClick={() => setDeletion(null)}>Cancelar</Button><Button variant="destructive" disabled={busy} onClick={() => deletion && void mutate("delete_project_lesson", { id: deletion.id, revision: deletion.revision }, "Aprendizado excluído")}>Excluir aprendizado</Button></DialogFooter></DialogContent></Dialog>
    <Dialog open={importText !== null} onOpenChange={open => { if (!open && !busy) setImportText(null); }}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"><DialogHeader><DialogTitle>Revisar importação</DialogTitle><DialogDescription>Os itens entram como sugestões. Ajuste os campos scope para pastas existentes neste projeto; use . para o projeto inteiro.</DialogDescription></DialogHeader>{error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}<Textarea aria-label="Aprendizados para importar" className="min-h-64 font-mono text-xs" value={importText ?? ""} onChange={e => setImportText(e.target.value)} /><DialogFooter><Button variant="outline" disabled={busy} onClick={() => setImportText(null)}>Cancelar</Button><Button disabled={busy} onClick={() => void mutate("import_project_learning", { content: importText }, "Aprendizados importados como sugestões")}>Importar sugestões</Button></DialogFooter></DialogContent></Dialog>
    <Dialog open={promotion !== null} onOpenChange={open => { if (!open && !busy) setPromotion(null); }}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle>Incorporar às regras do projeto</DialogTitle><DialogDescription>Revise o documento antes de salvar. O conteúdo atual será preservado e a lição será acrescentada.</DialogDescription></DialogHeader>{error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}{promotion && <div className="grid min-w-0 gap-4 sm:grid-cols-2"><div className="min-w-0 space-y-2"><Label htmlFor="learning-rules-before">Atual</Label><MarkdownEditor id="learning-rules-before" label="Atual" readOnly value={promotion.document.content} contentClassName="min-h-64" /></div><div className="min-w-0 space-y-2"><Label htmlFor="learning-rules-after">Após incorporar</Label><MarkdownEditor id="learning-rules-after" label="Após incorporar" value={promotion.content} disabled={busy} onChange={content => setPromotion({ ...promotion, content })} contentClassName="min-h-64" /></div></div>}<DialogFooter><Button variant="outline" disabled={busy} onClick={() => setPromotion(null)}>Cancelar</Button><Button disabled={busy} onClick={() => void savePromotion()}>Salvar regras</Button></DialogFooter></DialogContent></Dialog>
  </Card>;
}
