import { AlertCircle, BookOpen, Bot, Braces, Check, ChevronRight, Eye, FilePenLine, FileText, FolderSearch, GitBranch, Globe, ImagePlus, Layers3, ListTodo, Search, Stethoscope, Terminal, TriangleAlert, Wrench } from "lucide-react";
import { readWebSearchResult } from "@/core/web-search";
import { readVisionResult } from "@/core/attachments";
import { agentToolSchema } from "@/core/chat";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { lazy, Suspense, useState } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import type { ToolCallItem } from "./types";
import { QuestionHistory } from "./QuestionHistory";
import { isTaskReminder } from "./tool-activity";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));
const DEFERRED_DETAIL_KEY = "_jarvisHistoryDetailsDeferred";
const detailCache = new Map<string, Promise<ToolCallItem>>();
const MAX_CACHED_DETAILS = 32;
const MAX_CACHED_DETAIL_BYTES = 256 * 1024;

function hasDeferredDetails(tool: ToolCallItem) {
  return tool.args?.[DEFERRED_DETAIL_KEY] === true;
}

function loadToolDetails(context: { conversationId: string; turnId: string }, toolId: string) {
  const key = `${context.conversationId}:${context.turnId}:${toolId}`;
  const cached = detailCache.get(key);
  if (cached) return cached;
  const request = invoke<unknown>("get_chat_tool_call", { ...context, toolId }).then(value => {
    const detail = agentToolSchema.parse(value);
    const bytes = detail.output.length + JSON.stringify(detail.args).length;
    if (bytes > MAX_CACHED_DETAIL_BYTES) detailCache.delete(key);
    return detail;
  });
  detailCache.set(key, request);
  if (detailCache.size > MAX_CACHED_DETAILS) detailCache.delete(detailCache.keys().next().value ?? key);
  request.catch(() => detailCache.delete(key));
  return request;
}

const tools = {
  ctx_search: { label: "Context Mode · Busca", icon: Search },
  ctx_index: { label: "Context Mode · Indexação", icon: BookOpen },
  ctx_execute: { label: "Context Mode · Processamento", icon: Terminal },
  ctx_execute_file: { label: "Context Mode · Arquivo", icon: FileText },
  ctx_batch_execute: { label: "Context Mode · Lote", icon: Layers3 },
  ctx_fetch_and_index: { label: "Context Mode · Web", icon: Globe },
  ctx_stats: { label: "Context Mode · Economia", icon: Layers3 },
  read: { label: "Leitura de arquivo", icon: FileText },
  read_attachment: { label: "Leitura de anexo", icon: FileText },
  vision: { label: "Análise de imagem", icon: Eye },
  generate_image: { label: "Geração de imagem", icon: ImagePlus },
  read_skill: { label: "Leitura de skill", icon: BookOpen },
  context7_resolve_library_id: { label: "Bibliotecas · Context7", icon: Search },
  context7_query_docs: { label: "Documentação · Context7", icon: BookOpen },
  find_skills: { label: "Busca de skills", icon: BookOpen },
  jarvis_catalog: { label: "Catálogo do Jarvis", icon: Bot },
  jarvis_propose_agent: { label: "Proposta de agente", icon: Bot },
  jarvis_propose_flow: { label: "Proposta de fluxo", icon: Bot },
  design_search: { label: "Referências de design", icon: Search },
  design_read: { label: "Recurso de design", icon: BookOpen },
  design_brief: { label: "Briefing de design", icon: FileText },
  hub_request_guidance: { label: "Solicitar orientação", icon: Bot },
  hub_respond_guidance: { label: "Responder orientação", icon: Bot },
  list: { label: "Listagem de arquivos", icon: FolderSearch },
  search: { label: "Busca no projeto", icon: Search },
  web_search: { label: "Pesquisa na web", icon: Globe },
  browser_open: { label: "Abrir navegador", icon: Globe },
  browser_list: { label: "Abas do navegador", icon: Globe },
  browser_navigate: { label: "Navegar na página", icon: Globe },
  browser_snapshot: { label: "Ler página", icon: Eye },
  browser_console: { label: "Console do navegador", icon: Terminal },
  browser_screenshot: { label: "Capturar página", icon: Eye },
  browser_click: { label: "Clicar na página", icon: Globe },
  browser_fill: { label: "Preencher campo", icon: Globe },
  browser_press: { label: "Tecla na página", icon: Globe },
  browser_scroll: { label: "Rolar página", icon: Globe },
  browser_close: { label: "Fechar navegador", icon: Globe },
  write: { label: "Escrita de arquivo", icon: FilePenLine },
  edit: { label: "Edição de arquivo", icon: FilePenLine },
  apply_patch: { label: "Patch transacional", icon: GitBranch },
  lsp_definition: { label: "Código · Definição", icon: Braces },
  lsp_references: { label: "Código · Referências", icon: GitBranch },
  lsp_symbols: { label: "Código · Símbolos", icon: Braces },
  lsp_diagnostics: { label: "Código · Diagnósticos", icon: Stethoscope },
  bash: { label: "Execução no terminal", icon: Terminal },
  process_start: { label: "Iniciar processo", icon: Terminal },
  process_list: { label: "Consultar processos", icon: Terminal },
  process_output: { label: "Saída do processo", icon: Terminal },
  process_check_port: { label: "Verificar porta", icon: Terminal },
  validation_publish: { label: "Validação manual", icon: Check },
  hub_spawn: { label: "Delegar tarefa", icon: Bot },
  hub_list: { label: "Consultar agentes", icon: Bot },
  hub_wait: { label: "Aguardar agentes", icon: Bot },
  hub_send: { label: "Enviar orientação", icon: Bot },
  hub_retry: { label: "Retomar agente", icon: Bot },
  hub_cancel: { label: "Interromper agente", icon: Bot },
  hub_complete: { label: "Entregar resultado", icon: Check },
  workflow_check: { label: "Validar projeto", icon: Terminal },
  update_tasks: { label: "Atualizar tarefas", icon: ListTodo },
};

export function ToolCallCard({ tool, detailContext }: { tool: ToolCallItem; detailContext?: { conversationId: string; turnId: string } }) {
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState<ToolCallItem | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  if (tool.name === "ask_user") return <QuestionHistory tool={tool} />;
  const current = loaded ?? tool;
  const deferred = hasDeferredDetails(tool) && loaded === null;
  const { label, icon: Icon } = tools[current.name as keyof typeof tools] ?? { label: current.name, icon: Wrench };
  const detail = current.name === "read_skill" ? current.output?.match(/^Skill: (.+)/)?.[1] ?? current.args?.path : current.args?.title ?? current.args?.path ?? current.args?.command ?? current.args?.query ?? current.args?.question ?? current.args?.url;
  const searchResult = current.name === "web_search" && current.output ? readWebSearchResult(current.output) : null;
  const visionResult = current.name === "vision" && current.output ? readVisionResult(current.output) : null;
  const warning = isTaskReminder(current);
  const status = warning ? "Atenção" : { pending: "Aguardando autorização", running: "Executando", completed: "Concluída", error: "Não concluída" }[current.status];
  const mutation = ["write", "edit", "apply_patch"].includes(current.name);
  const requestDetails = () => {
    if (!deferred || !detailContext || loading) return;
    setLoading(true); setLoadError(null);
    void loadToolDetails(detailContext, tool.id).then(setLoaded).catch(() => setLoadError("Não foi possível carregar os detalhes desta ação.")).finally(() => setLoading(false));
  };
  return (
    <Collapsible open={open} onOpenChange={next => { setOpen(next); if (next) requestDetails(); }} data-testid={`tool-call-${tool.id}`} className="tool-slot min-w-0">
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group flex h-auto min-h-9 w-full cursor-pointer justify-start gap-2 px-2.5 text-left text-[11px]">
        <Icon aria-hidden="true" data-icon="inline-start" className={`size-3.5 shrink-0 ${mutation ? "text-onedark-yellow" : current.name === "bash" ? "text-onedark-green" : current.name.includes("skill") ? "text-onedark-purple" : current.name === "web_search" || current.name.startsWith("ctx_") ? "text-onedark-cyan" : "text-primary"}`} />
        <span className="min-w-0 flex-1 truncate" title={typeof detail === "string" ? detail : undefined}>
          <span className="text-muted-foreground">{label}</span>{typeof detail === "string" && <> <span className="mx-1 text-muted-foreground/50">/</span> <span className="font-mono text-[10px] text-foreground">{detail}</span></>}
        </span>
        <span className="sr-only">{status}</span>
        {loading || current.status === "running" || current.status === "pending" ? <Spinner aria-hidden="true" className="motion-reduce:animate-none" /> : warning ? <TriangleAlert aria-hidden="true" className="text-onedark-yellow" /> : current.status === "error" ? <AlertCircle aria-hidden="true" className="text-destructive" /> : <Check aria-hidden="true" className="text-onedark-green" />}
        <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </CollapsibleTrigger>
      <CollapsibleContent className="flex min-w-0 flex-col gap-3 border-t border-border p-3 text-xs">
        <p className="font-mono text-[10px] tabular-nums text-muted-foreground">{status}{current.durationMs !== undefined && ` · ${(current.durationMs / 1000).toLocaleString("pt-BR", { maximumFractionDigits: 2 })}s`}</p>
        {loading && <div role="status" aria-label="Carregando detalhes da ação" className="space-y-2"><Skeleton className="h-3 w-2/5" /><Skeleton className="h-16 w-full" /></div>}
        {loadError && <div className="flex items-center justify-between gap-3"><p role="alert" className="text-destructive">{loadError}</p><Button variant="outline" size="sm" className="cursor-pointer" onClick={requestDetails}>Tentar novamente</Button></div>}
        {!deferred && current.args && <div><p className="mb-1">Parâmetros</p><pre className="max-h-48 overflow-auto rounded-md bg-muted p-3 text-foreground">{JSON.stringify(current.args, null, 2)}</pre></div>}
        {searchResult ? <div className="flex max-h-80 flex-col gap-3 overflow-auto rounded-md bg-muted p-3 text-foreground">
          <p className="text-xs text-muted-foreground">{searchResult.accountAlias} · {searchResult.model}</p>
          <Suspense fallback={<Skeleton className="h-8 w-full" />}><ChatMarkdown content={searchResult.answer} /></Suspense>
          <p className="font-medium">Fontes</p>
          {searchResult.sources.map((source) => <Button key={source.url} variant="link" className="h-auto cursor-pointer justify-start whitespace-normal px-0 text-left text-xs" onClick={() => { void openUrl(source.url).catch(() => toast.error("Não foi possível abrir o link")); }}>{source.title}</Button>)}
        </div> : visionResult ? <div className="flex max-h-80 flex-col gap-3 overflow-auto rounded-md bg-muted p-3 text-foreground"><p className="font-mono text-[10px] text-muted-foreground">{visionResult.accountAlias} · {visionResult.model}</p><Suspense fallback={<Skeleton className="h-8 w-full" />}><ChatMarkdown content={visionResult.analysis} /></Suspense></div> : current.output && <div><p className="mb-1">Resultado</p><pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 text-foreground">{current.output}</pre></div>}
        {current.error && <p role={warning ? "status" : "alert"} className={`whitespace-pre-wrap ${warning ? "text-onedark-yellow" : "text-destructive"}`}>{current.error}</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}
