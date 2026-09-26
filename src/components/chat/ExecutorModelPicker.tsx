import { useState, type ComponentProps } from "react";
import { Check, ChevronDown, CircleHelp, Copy, ExternalLink, RefreshCw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { executorOf, claudeModels } from "@/core/executors";
import { writeClipboardText } from "@/core/clipboard";
import { useClaudeRuntime } from "@/hooks/use-claude-runtime";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Hint } from "@/components/ui/hint";
import { Skeleton } from "@/components/ui/skeleton";
import { ModelPicker } from "./ModelPicker";

export function ExecutorModelPicker({ nativeDisabled = false, ...props }: ComponentProps<typeof ModelPicker> & { nativeDisabled?: boolean }) {
  const executor = executorOf(props.selection);
  const runtime = useClaudeRuntime(executor === "claude");
  const [details, setDetails] = useState(false);
  const models = claudeModels(runtime.data);
  const status = runtime.loading ? "Consultando Claude Code…" : runtime.error ?? runtime.data?.error ?? (!runtime.data?.installed ? "Claude Code não encontrado" : !runtime.data.authenticated ? "Entre na sua conta pelo Claude Code" : "Claude Code conectado");
  return <>
    <DropdownMenu>
      <DropdownMenuTrigger disabled={props.disabled} aria-label={`Executor · ${props.ariaLabel ?? "chat"}`} className="flex h-7.5 shrink-0 cursor-pointer items-center gap-1 rounded-md px-2 font-mono text-[10px] hover:bg-secondary focus-visible:ring-1 focus-visible:ring-ring">
        {executor === "claude" ? "Claude" : "Jarvis"}<ChevronDown className="size-3 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuItem className="cursor-pointer justify-between gap-4" onClick={() => {
          if (executor === "jarvis") return;
          const first = props.modelGroups.flatMap(group => group.models)[0];
          if (!first) { toast.error("Conecte um provedor para usar o executor Jarvis."); return; }
          props.onSelect({ executor: "jarvis", model: first.value, reasoning: first.defaultReasoningLevel ?? first.reasoningLevels[0] ?? null });
        }}>Jarvis{executor === "jarvis" && <Check className="size-3" />}</DropdownMenuItem>
        <DropdownMenuItem className="cursor-pointer justify-between gap-4" onClick={() => {
          if (executor === "claude") return;
          const first = models.find(model => model.value === "default") ?? models[0];
          props.onSelect({ executor: "claude", model: first?.value ?? "default", reasoning: first?.defaultReasoningLevel ?? null });
        }}>Claude{executor === "claude" && <Check className="size-3" />}</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
    {executor === "jarvis" ? <ModelPicker {...props} disabled={props.disabled || nativeDisabled} onSelect={next => props.onSelect({ ...next, executor: "jarvis" })} /> : <>
      {runtime.loading && !runtime.data ? <Skeleton className="h-7.5 w-36" role="status" aria-label="Carregando modelos do Claude" /> : <ModelPicker {...props} modelGroups={models.length ? [{ provider: "Claude Code", executor: "claude", models }] : []} showProviderIdentity={false} disabled={props.disabled || runtime.loading} onSelect={next => props.onSelect({ ...next, executor: "claude" })} onRefresh={() => { void runtime.refresh(); }} refreshing={runtime.loading} emptyMessage={status} />}
      <Hint content={status}><Button type="button" size="icon-sm" variant="ghost" aria-label="Status e configuração do Claude Code" className="shrink-0 cursor-pointer" onClick={() => setDetails(true)}><CircleHelp className="size-3.5" /></Button></Hint>
    </>}
    <Dialog open={details} onOpenChange={setDetails}><DialogContent className="dark sm:max-w-md">
      <DialogHeader><DialogTitle>Claude Code</DialogTitle><DialogDescription>O Claude executa com sua instalação e autenticação próprias. A conta e a cobrança seguem a configuração do CLI.</DialogDescription></DialogHeader>
      {runtime.loading && !runtime.data ? <Skeleton className="h-16" aria-label="Consultando Claude Code" /> : <div className="flex flex-col gap-2 text-xs"><p role="status">{status}</p>{runtime.data?.version && <p className="font-mono text-muted-foreground">Versão {runtime.data.version}</p>}{runtime.data?.email && <p>{runtime.data.email}</p>}{runtime.data?.authMethod && <p className="text-muted-foreground">Autenticação: {runtime.data.authMethod}{runtime.data.subscriptionType ? ` · ${runtime.data.subscriptionType}` : ""}</p>}</div>}
      <p className="text-xs text-muted-foreground">Para entrar ou trocar de conta, execute <code className="font-mono text-foreground">claude auth login</code> no terminal. Depois, atualize o status.</p>
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="outline" size="sm" onClick={() => { void writeClipboardText("claude auth login").then(() => toast.success("Comando copiado")).catch(() => toast.error("Não foi possível copiar o comando.")); }}><Copy className="size-3.5" />Copiar comando de login</Button>
        <Button type="button" variant="outline" size="sm" disabled={runtime.loading} onClick={() => { void runtime.refresh(); }}><RefreshCw className={`size-3.5 ${runtime.loading ? "animate-spin motion-reduce:animate-none" : ""}`} />Atualizar status e modelos</Button>
        <Button type="button" variant="link" size="sm" onClick={() => { void openUrl("https://code.claude.com/docs/en/setup").catch(() => toast.error("Não foi possível abrir a documentação.")); }}><ExternalLink className="size-3.5" />Instalar Claude Code</Button>
      </div>
    </DialogContent></Dialog>
  </>;
}
