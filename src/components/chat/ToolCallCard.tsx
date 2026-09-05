import { AlertCircle, Check, ChevronRight, FilePenLine, FileText, FolderSearch, Globe, Search, Terminal, Wrench } from "lucide-react";
import { readWebSearchResult } from "@/core/web-search";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { lazy, Suspense } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import type { ToolCallItem } from "./types";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));

const tools = {
  read: { label: "Leitura de arquivo", icon: FileText },
  list: { label: "Listagem de arquivos", icon: FolderSearch },
  search: { label: "Busca no projeto", icon: Search },
  web_search: { label: "Pesquisa na web", icon: Globe },
  write: { label: "Escrita de arquivo", icon: FilePenLine },
  edit: { label: "Edição de arquivo", icon: FilePenLine },
  bash: { label: "Execução no terminal", icon: Terminal },
};

export function ToolCallCard({ tool }: { tool: ToolCallItem }) {
  const { label, icon: Icon } = tools[tool.name as keyof typeof tools] ?? { label: tool.name, icon: Wrench };
  const detail = tool.args?.path ?? tool.args?.command ?? tool.args?.query;
  const searchResult = tool.name === "web_search" && tool.output ? readWebSearchResult(tool.output) : null;
  const status = { pending: "Aguardando autorização", running: "Executando", completed: "Concluída", error: "Não concluída" }[tool.status];
  return (
    <Collapsible data-testid={`tool-call-${tool.id}`} className="min-w-0">
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group flex h-auto min-h-8 w-full cursor-pointer justify-start gap-2 px-1 text-left">
        <Icon aria-hidden="true" data-icon="inline-start" />
        <span className="min-w-0 flex-1 truncate" title={typeof detail === "string" ? detail : undefined}>
          {label}{typeof detail === "string" && <> · <span className="text-foreground">{detail}</span></>}
        </span>
        <span className="sr-only">{status}</span>
        {tool.status === "running" || tool.status === "pending" ? <Spinner aria-hidden="true" className="motion-reduce:animate-none" /> : tool.status === "error" ? <AlertCircle aria-hidden="true" className="text-destructive" /> : <Check aria-hidden="true" className="text-onedark-green" />}
        <ChevronRight aria-hidden="true" data-icon="inline-end" className="transition-transform group-aria-expanded:rotate-90 motion-reduce:transition-none" />
      </CollapsibleTrigger>
      <CollapsibleContent className="flex min-w-0 flex-col gap-3 py-2 pl-6 text-xs">
        <p>{status}{tool.durationMs !== undefined && ` · ${(tool.durationMs / 1000).toLocaleString("pt-BR", { maximumFractionDigits: 2 })}s`}</p>
        {tool.args && <div><p className="mb-1">Parâmetros</p><pre className="max-h-48 overflow-auto rounded-md bg-muted p-3 text-foreground">{JSON.stringify(tool.args, null, 2)}</pre></div>}
        {searchResult ? <div className="flex max-h-80 flex-col gap-3 overflow-auto rounded-md bg-muted p-3 text-foreground">
          <p className="text-xs text-muted-foreground">{searchResult.accountAlias} · {searchResult.model}</p>
          <Suspense fallback={<Skeleton className="h-8 w-full" />}><ChatMarkdown content={searchResult.answer} /></Suspense>
          <p className="font-medium">Fontes</p>
          {searchResult.sources.map((source) => <Button key={source.url} variant="link" className="h-auto cursor-pointer justify-start whitespace-normal px-0 text-left text-xs" onClick={() => { void openUrl(source.url).catch(() => toast.error("Não foi possível abrir o link")); }}>{source.title}</Button>)}
        </div> : tool.output && <div><p className="mb-1">Resultado</p><pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 text-foreground">{tool.output}</pre></div>}
        {tool.error && <p role="alert" className="whitespace-pre-wrap text-destructive">{tool.error}</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}
