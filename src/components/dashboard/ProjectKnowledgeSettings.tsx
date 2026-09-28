import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BookOpen, ChevronDown, Download, Link, RefreshCw, Save, Sparkles, Upload } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { ExecutorModelPicker } from "@/components/chat/ExecutorModelPicker";
import { accountGroups } from "@/components/settings/workflow/workflow-models";
import { BootstrapResourcesContext } from "@/core/bootstrap-context";
import { executionChoice, type ExecutionSelection } from "@/core/executors";
import { libraryError } from "@/core/library";
import { KNOWLEDGE_KINDS, knowledgeChanged, knowledgeDocumentSchema, knowledgeDraftSchema, knowledgeKey, knowledgeSnapshotSchema, type KnowledgeDocument, type KnowledgeDraft, type KnowledgeKind, type KnowledgeSnapshot } from "@/core/project-knowledge";

const phases: Record<string, string> = { scanning: "Analisando arquivos do projeto…", generating: "Gerando rascunho…", reconnecting: "Reconectando ao provedor…", cancelling: "Cancelando geração…" };

export function ProjectKnowledgeSettings({ projectId }: { projectId: string }) {
  const bootstrap = useContext(BootstrapResourcesContext);
  const [snapshot, setSnapshot] = useState<KnowledgeSnapshot | null>(null);
  const [drafts, setDrafts] = useState<Record<string, KnowledgeDocument>>({});
  const [scope, setScope] = useState(".");
  const [kind, setKind] = useState<KnowledgeKind>("product");
  const [selection, setSelection] = useState<ExecutionSelection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [phase, setPhase] = useState<string | null>(null);
  const [preview, setPreview] = useState<(KnowledgeDraft & { title: string }) | null>(null);
  const [linkOpen, setLinkOpen] = useState(false);
  const [linkPath, setLinkPath] = useState("");
  const request = useRef(0);
  const job = useRef<string | null>(null);
  const mounted = useRef(true);

  const reload = useCallback(() => {
    const id = ++request.current;
    return invoke("get_project_knowledge", { projectId }).then(value => {
      const next = knowledgeSnapshotSchema.parse(value);
      if (id === request.current && mounted.current) { setSnapshot(next); setError(null); }
    }).catch(cause => {
      if (id === request.current && mounted.current) setError(libraryError(cause, "Não foi possível carregar o conhecimento do projeto."));
    }).finally(() => {
      if (id === request.current && mounted.current) setLoading(false);
    });
  }, [projectId]);

  useEffect(() => {
    mounted.current = true;
    void reload();
    const events = listen<{ id: string; phase: string }>("project:knowledge-generation", ({ payload }) => {
      if (mounted.current && payload.id === job.current) setPhase(payload.phase);
    }).catch(() => () => {});
    const repositories = listen<string>("project:repositories-changed", ({ payload }) => {
      if (mounted.current && payload === projectId) { setLoading(true); void reload(); }
    }).catch(() => () => {});
    return () => {
      mounted.current = false; request.current += 1;
      void events.then(unlisten => unlisten()).catch(() => {});
      void repositories.then(unlisten => unlisten()).catch(() => {});
      if (job.current) void invoke("cancel_project_knowledge_generation", { id: job.current }).catch(() => {});
      job.current = null;
    };
  }, [reload, projectId]);

  const key = knowledgeKey({ scope, kind });
  const saved = snapshot?.documents.find(document => knowledgeKey(document) === key);
  const document = drafts[key] ?? saved;
  const changed = !!saved && !!document && knowledgeChanged(document, saved);
  const conflict = !!saved && !!document && changed && saved.revision !== document.revision;
  const tooLarge = !!document && new TextEncoder().encode(document.content).length > 65_536;
  const busy = saving || phase !== null;
  const edit = (patch: Partial<KnowledgeDocument>) => {
    if (document) setDrafts(current => ({ ...current, [key]: { ...document, ...patch } }));
  };
  const acceptSaved = (next: KnowledgeDocument) => {
    setSnapshot(current => current ? { ...current, documents: current.documents.map(item => knowledgeKey(item) === knowledgeKey(next) ? next : item) } : current);
    setDrafts(current => { const remaining = { ...current }; delete remaining[knowledgeKey(next)]; return remaining; });
  };

  const save = async () => {
    if (!document || busy) return;
    setSaving(true); setError(null);
    try {
      const { kind, scope, content, essential, revision, sources } = document;
      const next = knowledgeDocumentSchema.parse(await invoke("save_project_knowledge", { projectId, document: { kind, scope, content, essential, revision, sources } }));
      if (!mounted.current) return;
      acceptSaved(next); toast.success("Conhecimento salvo");
    } catch (cause) {
      if (mounted.current) setError(libraryError(cause, "Não foi possível salvar. Suas edições continuam no editor."));
    } finally { if (mounted.current) setSaving(false); }
  };

  const generate = async () => {
    if (!selection || !saved || busy) return;
    const id = crypto.randomUUID();
    job.current = id; setPhase("scanning"); setError(null);
    try {
      const result = knowledgeDraftSchema.parse(await invoke("generate_project_knowledge", { request: { id, projectId, scope, kind, choice: executionChoice(selection) } }));
      if (mounted.current && job.current === id) setPreview({ ...result, title: "Rascunho gerado" });
    } catch (cause) {
      if (mounted.current && job.current === id) setError(libraryError(cause, "Não foi possível gerar o rascunho. O documento atual foi preservado."));
    } finally {
      if (mounted.current && job.current === id) { job.current = null; setPhase(null); }
    }
  };
  const cancel = async () => {
    const id = job.current;
    if (!id) return;
    job.current = null; setPhase("cancelling");
    try {
      await invoke("cancel_project_knowledge_generation", { id });
      if (mounted.current) toast.success("Geração cancelada; documento preservado");
    } catch (cause) { if (mounted.current) setError(libraryError(cause, "Não foi possível cancelar a geração.")); }
    finally { if (mounted.current) setPhase(null); }
  };
  const importMarkdown = async () => {
    if (!document) return;
    setSaving(true);
    try {
      const content = await invoke<string | null>("import_project_knowledge");
      if (content !== null && mounted.current) setPreview({ content, revision: document.revision, sources: [], title: "Markdown importado" });
    } catch (cause) { if (mounted.current) setError(libraryError(cause, "Não foi possível importar o documento.")); }
    finally { if (mounted.current) setSaving(false); }
  };
  const link = async () => {
    if (!saved) return;
    setSaving(true);
    try {
      const next = knowledgeDocumentSchema.parse(await invoke("link_project_knowledge", { projectId, scope, kind, path: linkPath.trim(), revision: saved.revision }));
      if (mounted.current) { acceptSaved(next); setLinkOpen(false); setError(null); toast.success("Documento vinculado"); }
    } catch (cause) { if (mounted.current) toast.error(libraryError(cause, "Não foi possível vincular o documento.")); }
    finally { if (mounted.current) setSaving(false); }
  };
  const exportMarkdown = async () => {
    try {
      const exported = await invoke<boolean>("save_markdown_document", { content: document?.content ?? "", suggestedFileName: KNOWLEDGE_KINDS[kind].file });
      if (exported) toast.success("Markdown exportado");
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível exportar o documento.")); }
  };

  return <Card className="min-w-0">
    <CardHeader className="border-b border-border">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <CardTitle className="flex items-center gap-2"><BookOpen className="size-4 text-onedark-cyan" />Conhecimento do projeto</CardTitle>
        <Button variant="ghost" size="sm" disabled={loading || busy} onClick={() => { setLoading(true); void reload(); }}><RefreshCw className="size-3.5" />Recarregar documentos</Button>
      </div>
      <CardDescription>Uma base editável para os agentes consultarem somente os trechos relevantes. Regras essenciais entram automaticamente no contexto do escopo correspondente.</CardDescription>
    </CardHeader>
    <CardContent className="min-w-0 space-y-4 pt-5">
      {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
      {!snapshot && loading && <div role="status" aria-label="Carregando conhecimento"><Skeleton className="mb-3 h-9 w-60" /><Skeleton className="h-64 w-full" /></div>}
      {snapshot && <>
        <div className="max-w-md space-y-2">
          <Label htmlFor="knowledge-scope">Escopo do conhecimento</Label>
          <Select value={scope} disabled={busy || !!preview} items={snapshot.scopes.map(value => ({ value, label: value === "." ? "Projeto inteiro · compartilhado" : value }))} onValueChange={value => { if (value) { setScope(value); setError(null); } }}>
            <SelectTrigger id="knowledge-scope" className="w-full min-w-0"><SelectValue /></SelectTrigger>
            <SelectContent>{snapshot.scopes.map(value => <SelectItem key={value} value={value}>{value === "." ? "Projeto inteiro · compartilhado" : value}</SelectItem>)}</SelectContent>
          </Select>
        </div>
        <Tabs value={kind} onValueChange={value => { setKind(value as KnowledgeKind); setError(null); }}>
          <TabsList className="h-auto w-full flex-wrap justify-start">{Object.entries(KNOWLEDGE_KINDS).map(([value, item]) => <TabsTrigger key={value} value={value} disabled={busy || !!preview} className="cursor-pointer">{item.label}</TabsTrigger>)}</TabsList>
        {document && <TabsContent value={kind} className="min-w-0 space-y-4">
          <div className="space-y-1">
            <div className="flex flex-wrap items-center gap-2"><Label htmlFor="knowledge-content">{KNOWLEDGE_KINDS[kind].label} · Markdown</Label>{changed && <Badge variant="outline" className="text-onedark-yellow">Não salvo</Badge>}</div>
            <p className="text-xs text-muted-foreground">{KNOWLEDGE_KINDS[kind].description}</p>
            <p className="break-all font-mono text-[10px] text-muted-foreground">{document.path}</p>
          </div>
          {document.error && <Alert variant="destructive"><AlertDescription>{document.error} Corrija o arquivo e recarregue os documentos.</AlertDescription></Alert>}
          <Textarea id="knowledge-content" value={document.content} disabled={saving || !!document.error} onChange={event => edit({ content: event.target.value })} spellCheck={false} className="min-h-64 resize-y font-mono text-xs leading-5" placeholder="Escreva o conhecimento, vincule um documento existente ou gere um rascunho com IA." />
          {tooLarge && <p role="alert" className="text-xs text-destructive">O documento excede 64 KiB. Reduza o conteúdo antes de salvar.</p>}
          {kind === "rules" && <div className="space-y-2">
            <Label htmlFor="knowledge-essential">Regras essenciais</Label>
            <Textarea id="knowledge-essential" value={document.essential} maxLength={2000} disabled={saving || !!document.error} onChange={event => edit({ essential: event.target.value })} className="min-h-20 text-xs" />
            <p className="text-xs text-muted-foreground">Até 2.000 caracteres sempre disponíveis para o escopo. Use apenas regras indispensáveis; detalhes ficam no Markdown. As instruções atuais do usuário e o AGENTS.md têm prioridade.</p>
          </div>}
          {!!document.staleSources.length && <Alert><AlertDescription>Fontes alteradas desde a análise: {document.staleSources.join(", ")}. Revise as afirmações ou gere um novo rascunho.</AlertDescription></Alert>}
          {conflict && <Alert><AlertDescription>A versão salva mudou. Confira o documento salvo abaixo antes de manter suas edições sobre essa versão.<Button size="sm" variant="outline" disabled={busy} onClick={() => edit({ revision: saved.revision })}>Manter minhas edições sobre a versão atual</Button></AlertDescription></Alert>}
          {(changed || !!document.sources.length) && <Collapsible>
            <CollapsibleTrigger className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground"><ChevronDown className="size-3.5" />Documento salvo e fontes</CollapsibleTrigger>
            <CollapsibleContent className="space-y-3 pt-3">
              {changed && <Textarea aria-label="Documento salvo" value={saved?.content ?? ""} readOnly className="min-h-32 font-mono text-xs" />}
              {!!document.sources.length && <ul className="space-y-1 break-all font-mono text-[10px] text-muted-foreground">{document.sources.map(source => <li key={source.path}>{source.path}</li>)}</ul>}
            </CollapsibleContent>
          </Collapsible>}
          <div className="flex flex-wrap items-center gap-2 border-y border-border py-3">
            <ExecutorModelPicker selection={selection} modelGroups={accountGroups(bootstrap?.resources.accounts ?? [])} onSelect={setSelection} disabled={busy} showProviderIdentity ariaLabel="Modelo para gerar conhecimento" />
            <Button size="sm" variant="outline" disabled={busy || !selection || !!document.error} onClick={() => { void generate(); }}><Sparkles className="size-3.5" />Analisar e gerar {KNOWLEDGE_KINDS[kind].label.toLowerCase()}</Button>
            {phase && <div role="status" className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"><Spinner className="size-3.5" />{phases[phase] ?? phases.generating}<Button size="sm" variant="ghost" onClick={() => { void cancel(); }}>Cancelar geração</Button></div>}
            <p className="basis-full text-[11px] text-muted-foreground">Analisa uma amostra dos arquivos locais e a envia ao modelo selecionado. Limite de 3 minutos. O resultado é um rascunho; nada é salvo automaticamente.</p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" disabled={busy || !!document.error} onClick={() => { void importMarkdown(); }}><Upload className="size-3.5" />Importar MD</Button>
            <Button size="sm" variant="outline" disabled={busy || !document.content} onClick={() => { void exportMarkdown(); }}><Download className="size-3.5" />Exportar MD</Button>
            <Button size="sm" variant="outline" disabled={busy || changed || !!document.error} onClick={() => { setLinkPath(""); setLinkOpen(true); }}><Link className="size-3.5" />Vincular existente</Button>
            <div className="ml-auto flex gap-2">
              {changed && <Button size="sm" variant="ghost" disabled={busy} onClick={() => { if (saved) acceptSaved(saved); }}>Descartar rascunho</Button>}
              <Button size="sm" disabled={busy || !changed || tooLarge || conflict || !!document.error} onClick={() => { void save(); }}><Save className="size-3.5" />{saving ? "Salvando…" : "Salvar conhecimento"}</Button>
            </div>
          </div>
          <p className="text-[11px] leading-5 text-muted-foreground">O Markdown fica na pasta do projeto; um documento vinculado é editado no próprio arquivo. Inclua esses arquivos e a pasta .jarvis/knowledge no backup ou Git do projeto. O backup global de configurações não inclui estes documentos.</p>
        </TabsContent>}
        </Tabs>
      </>}
    </CardContent>

    <Dialog open={!!preview} onOpenChange={open => { if (!open) setPreview(null); }}>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader><DialogTitle>{preview?.title}</DialogTitle><DialogDescription>Confira fatos, inferências e fontes. Este conteúdo ainda não está salvo.{changed ? " Usar este rascunho substituirá o texto atual do editor." : ""}</DialogDescription></DialogHeader>
        <Textarea aria-label="Prévia do rascunho" value={preview?.content ?? ""} onChange={event => setPreview(current => current ? { ...current, content: event.target.value } : current)} className="min-h-72 font-mono text-xs leading-5" />
        <Collapsible><CollapsibleTrigger className="flex cursor-pointer items-center gap-2 text-xs"><ChevronDown className="size-3.5" />Comparar com o editor atual</CollapsibleTrigger><CollapsibleContent className="pt-3"><Textarea aria-label="Texto atual do editor" readOnly value={document?.content ?? ""} className="min-h-32 font-mono text-xs" /></CollapsibleContent></Collapsible>
        <DialogFooter><Button variant="outline" onClick={() => setPreview(null)}>Descartar prévia</Button><Button onClick={() => { if (preview) { edit({ content: preview.content, revision: preview.revision, sources: preview.sources, staleSources: [] }); setPreview(null); } }}>Usar no editor</Button></DialogFooter>
      </DialogContent>
    </Dialog>
    <Dialog open={linkOpen} onOpenChange={open => { if (!saving) setLinkOpen(open); }}>
      <DialogContent>
        <DialogHeader><DialogTitle>Vincular documento existente</DialogTitle><DialogDescription>Use um Markdown da pasta do projeto como fonte desta categoria. O arquivo anterior será preservado. Ao salvar futuras edições, o Jarvis atualizará o arquivo vinculado.</DialogDescription></DialogHeader>
        <Label htmlFor="knowledge-link">Caminho relativo ao projeto</Label><Input id="knowledge-link" placeholder="docs/arquitetura.md" value={linkPath} onChange={event => setLinkPath(event.target.value)} disabled={saving} />
        <DialogFooter><Button variant="outline" disabled={saving} onClick={() => setLinkOpen(false)}>Cancelar</Button><Button disabled={saving || !linkPath.trim()} onClick={() => { void link(); }}>Vincular documento</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  </Card>;
}
