import { useState } from "react";
import { ChevronRight, Copy, ExternalLink, RefreshCw, TerminalSquare } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { claudeModels, DEFAULT_CLAUDE_PREFERENCES, type ClaudeProviderPreferences } from "@/core/executors";
import { writeClipboardText } from "@/core/clipboard";
import { libraryError } from "@/core/library";
import { useClaudeRuntime } from "@/hooks/use-claude-runtime";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";

function ClaudeProviderDetails() {
  const runtime = useClaudeRuntime();
  const preferences = runtime.data?.preferences ?? DEFAULT_CLAUDE_PREFERENCES;
  const busy = runtime.loading || runtime.saving;
  const save = async (next: ClaudeProviderPreferences) => {
    try {
      await runtime.savePreferences(next);
      toast.success("Preferências do Claude Code salvas");
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível salvar as preferências do Claude Code.")); }
  };
  const error = runtime.error ?? runtime.data?.error;
  return <>
    <DialogHeader>
      <DialogTitle className="flex items-center gap-2"><TerminalSquare className="size-5" aria-hidden="true" />Claude Code</DialogTitle>
      <DialogDescription>Provedor local único. Requer o CLI oficial instalado e autenticado nesta máquina. A conta, os limites e a cobrança seguem a configuração do Claude Code.</DialogDescription>
    </DialogHeader>
    <div className="flex min-h-0 flex-col gap-4 overflow-y-auto py-1">
      {runtime.loading && !runtime.data ? <Skeleton className="h-20" role="status" aria-label="Consultando Claude Code" /> : <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-xs">
        <dt className="text-muted-foreground">Instalação</dt><dd className="text-right font-mono">{runtime.data?.installed ? runtime.data.version ?? "Encontrada" : "Não encontrada"}</dd>
        <dt className="text-muted-foreground">Conta</dt><dd className="break-all text-right">{runtime.data?.authenticated ? runtime.data.email ?? "Autenticada no CLI" : "Login necessário"}</dd>
        {runtime.data?.authMethod && <><dt className="text-muted-foreground">Autenticação</dt><dd className="text-right">{runtime.data.authMethod}{runtime.data.subscriptionType ? ` · ${runtime.data.subscriptionType}` : ""}</dd></>}
      </dl>}
      {error && <Alert><AlertDescription>{error}</AlertDescription></Alert>}
      <p className="text-xs leading-5 text-muted-foreground">Para entrar ou trocar de conta, execute <code className="font-mono text-foreground">claude auth login</code> no terminal e atualize o status. O Jarvis usa esta instalação; não é necessário informar uma chave de API.</p>
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="outline" size="sm" onClick={() => { void writeClipboardText("claude auth login").then(() => toast.success("Comando copiado")).catch(() => toast.error("Não foi possível copiar o comando.")); }}><Copy data-icon="inline-start" />Copiar comando de login</Button>
        <Button type="button" variant="outline" size="sm" disabled={busy} onClick={() => { void runtime.refresh(); }}><RefreshCw data-icon="inline-start" className={runtime.loading ? "animate-spin motion-reduce:animate-none" : undefined} />Atualizar status e modelos</Button>
        <Button type="button" variant="link" size="sm" onClick={() => { void openUrl("https://code.claude.com/docs/en/setup").catch(() => toast.error("Não foi possível abrir a documentação.")); }}><ExternalLink data-icon="inline-start" />Instalar Claude Code</Button>
      </div>
      <Separator />
      <label className="flex cursor-pointer items-center justify-between gap-3 text-xs"><span aria-hidden="true">Disponibilizar Claude Code no Jarvis</span><Switch aria-label="Ativar Claude Code" checked={preferences.enabled} disabled={busy || !runtime.data} onCheckedChange={enabled => { void save({ ...preferences, enabled }); }} /></label>
      <label className="flex cursor-pointer items-center justify-between gap-3 text-xs"><span aria-hidden="true"><span className="block">Limites na barra de status</span><span className="mt-1 block text-muted-foreground">Exibe as cotas da assinatura quando a conta as disponibiliza.</span></span><Switch aria-label="Mostrar limites do Claude Code" checked={preferences.showUsage !== false} disabled={busy || !runtime.data || !preferences.enabled} onCheckedChange={showUsage => { void save({ ...preferences, showUsage }); }} /></label>
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2"><span className="micro-label text-muted-foreground">Modelos para seleção</span><Badge variant="secondary">{claudeModels(runtime.data).length}/{runtime.data?.models.length ?? 0}</Badge></div>
        <p className="text-xs leading-5 text-muted-foreground">Modelos novos ficam disponíveis automaticamente. Ao ocultar um modelo, revise os agentes e fluxos que o utilizam. Conversas em execução continuam normalmente.</p>
        {!runtime.loading && !runtime.data?.installed && <p role="status" className="text-xs text-muted-foreground">Claude Code não encontrado. Instale o CLI para consultar os modelos.</p>}
        {runtime.data?.installed && !runtime.data.authenticated && <p role="status" className="text-xs text-muted-foreground">Entre na sua conta pelo Claude Code para usar os modelos.</p>}
        <div className="divide-y divide-border">
          {runtime.data?.models.map(model => <label key={model.id} className="flex cursor-pointer items-center justify-between gap-3 py-2.5 text-xs">
            <span className="min-w-0" aria-hidden="true"><span className="block font-medium">{model.name}</span><span className="break-all font-mono text-[10px] text-muted-foreground">{model.id}</span></span>
            <Switch aria-label={`Disponibilizar ${model.name}`} disabled={busy || !preferences.enabled} checked={!preferences.disabledModels.includes(model.id)} onCheckedChange={enabled => { void save({ ...preferences, disabledModels: enabled ? preferences.disabledModels.filter(id => id !== model.id) : [...preferences.disabledModels, model.id] }); }} />
          </label>)}
        </div>
      </div>
    </div>
  </>;
}

export function ClaudeProviderDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-xl"><ClaudeProviderDetails /></DialogContent></Dialog>;
}

export function ClaudeProviderCard() {
  const runtime = useClaudeRuntime();
  const [open, setOpen] = useState(false);
  const enabled = runtime.data?.preferences?.enabled !== false;
  const status = runtime.loading ? "Consultando…" : !enabled ? "Desativado" : runtime.error || runtime.data?.error ? "Verificar configuração" : !runtime.data?.installed ? "Instalação necessária" : !runtime.data.authenticated ? "Login necessário" : "Conectado";
  return <Dialog open={open} onOpenChange={setOpen}>
    <Card size="sm" className="min-w-0 gap-0 py-0" data-testid="provider-account-claude-code">
      <DialogTrigger render={<CardHeader />} nativeButton={false} aria-label="Detalhes de Claude Code" className="cursor-pointer rounded-lg py-3 transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset">
        <div className="flex min-w-0 items-center gap-3">
          <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"><TerminalSquare className="size-4" aria-hidden="true" /></span>
          <div className="flex min-w-0 flex-1 flex-col gap-1"><CardTitle>Claude Code</CardTitle><div className="flex flex-wrap items-center gap-2"><Badge variant="outline">{status}</Badge><CardDescription>CLI local · {claudeModels(runtime.data).length} modelos ativos</CardDescription></div></div>
          <ChevronRight className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        </div>
      </DialogTrigger>
    </Card>
    <DialogContent className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-xl"><ClaudeProviderDetails /></DialogContent>
  </Dialog>;
}
