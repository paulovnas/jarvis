import { useEffect, useRef, useState } from "react";
import { z } from "zod";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight, Copy, ExternalLink, RefreshCw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { museModels, DEFAULT_MUSE_PREFERENCES, type MuseProviderPreferences } from "@/core/executors";
import { writeClipboardText } from "@/core/clipboard";
import { libraryError } from "@/core/library";
import { useMuseRuntime } from "@/hooks/use-muse-runtime";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { ProviderIcon } from "@/components/ProviderIcon";
import { ProviderUsageWindowSettings } from "./ProviderUsageWindowSettings";

const loginSchema = z.object({
  id: z.string().min(1), verificationUrl: z.url().refine(value => { const url = new URL(value); return url.protocol === "https:" && !url.username && !url.password; }),
  userCode: z.string().min(1), expiresAt: z.number().finite(),
});

function MuseProviderDetails() {
  const runtime = useMuseRuntime();
  const [login, setLogin] = useState<z.infer<typeof loginSchema> | null>(null);
  const [starting, setStarting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const attempt = useRef(0);
  const pendingId = useRef<string | null>(null);
  useEffect(() => () => {
    attempt.current++;
    if (pendingId.current) void invoke("cancel_muse_login", { id: pendingId.current }).catch(() => {});
  }, []);
  useEffect(() => {
    if (!runtime.data?.authenticated) return;
    attempt.current++;
    pendingId.current = null;
  }, [runtime.data?.authenticated]);
  if (runtime.data?.authenticated && (login || starting)) {
    setLogin(null);
    setStarting(false);
  }
  const preferences = runtime.data?.preferences ?? DEFAULT_MUSE_PREFERENCES;
  const busy = runtime.loading || runtime.saving || disconnecting;
  const connect = async () => {
    const request = ++attempt.current;
    setStarting(true);
    try {
      const next = loginSchema.parse(await invoke("start_muse_login"));
      if (request !== attempt.current) { await invoke("cancel_muse_login", { id: next.id }); return; }
      pendingId.current = next.id;
      setLogin(next);
      await openUrl(next.verificationUrl);
    } catch (cause) { if (request === attempt.current) toast.error(libraryError(cause, "Não foi possível iniciar o login do Muse Code.")); }
    finally { if (request === attempt.current) setStarting(false); }
  };
  const cancel = async () => {
    attempt.current++;
    const id = pendingId.current;
    pendingId.current = null;
    setLogin(null);
    setStarting(false);
    if (id) {
      try { await invoke("cancel_muse_login", { id }); }
      catch (cause) { toast.error(libraryError(cause, "Não foi possível confirmar o cancelamento do login.")); }
    }
  };
  const disconnect = async () => {
    setDisconnecting(true);
    try { await invoke("disconnect_muse_account"); toast.success("Muse Code desconectado"); }
    catch (cause) { toast.error(libraryError(cause, "Não foi possível desconectar o Muse Code.")); }
    finally { setDisconnecting(false); }
  };
  const save = async (next: MuseProviderPreferences) => {
    try {
      await runtime.savePreferences(next);
      toast.success("Preferências do Muse Code salvas");
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível salvar as preferências do Muse Code.")); }
  };
  const error = runtime.error ?? runtime.data?.error;
  return <>
    <DialogHeader className="min-w-0 shrink-0 pr-6">
      <DialogTitle className="flex items-center gap-2"><ProviderIcon kind="muse-code" className="size-5" />Muse Code</DialogTitle>
      <DialogDescription>Requer o CLI oficial e uma assinatura Muse Code ativa. A conexão usa um perfil privado do Jarvis.</DialogDescription>
    </DialogHeader>
    <div role="region" aria-label="Configurações do Muse Code" className="-mx-4 flex min-h-0 min-w-0 flex-col gap-4 overflow-x-hidden overflow-y-auto px-4 py-1 wrap-anywhere [scrollbar-gutter:stable]">
      {runtime.loading && !runtime.data ? <Skeleton className="h-20" role="status" aria-label="Consultando Muse Code" /> : <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-xs">
        <dt className="text-muted-foreground">Instalação</dt><dd className="text-right font-mono">{runtime.data?.installed ? runtime.data.version ?? "Encontrada" : "Não encontrada"}</dd>
        <dt className="text-muted-foreground">Conta</dt><dd className="break-all text-right">{runtime.data?.authenticated ? runtime.data.email ?? "Autenticada no Jarvis" : "Login necessário"}</dd>
        {runtime.data?.authMethod && <><dt className="text-muted-foreground">Autenticação</dt><dd className="text-right">{runtime.data.authMethod}{runtime.data.subscriptionType ? ` · ${runtime.data.subscriptionType}` : ""}</dd></>}
      </dl>}
      {error && <Alert><AlertDescription>{error}</AlertDescription></Alert>}
      <Alert><AlertDescription>Esta conexão usa a assinatura Muse Code. Uma chave da API Meta paga por uso não conecta a assinatura.</AlertDescription></Alert>
      {login && !runtime.data?.authenticated && <div role="status" className="space-y-2 text-xs">
        <p>Conclua o login no navegador com este código:</p>
        <p className="font-mono text-sm font-medium">{login.userCode}</p>
        <p className="text-muted-foreground">Conclua no Jarvis até {new Date(login.expiresAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}.</p>
        <div className="flex flex-wrap gap-2"><Button type="button" variant="outline" size="sm" onClick={() => { void writeClipboardText(login.userCode).then(() => toast.success("Código copiado")).catch(() => toast.error("Não foi possível copiar o código.")); }}><Copy data-icon="inline-start" />Copiar código</Button><Button type="button" variant="outline" size="sm" onClick={() => { void openUrl(login.verificationUrl).catch(() => toast.error("Não foi possível abrir o navegador.")); }}><ExternalLink data-icon="inline-start" />Abrir login no navegador</Button></div>
      </div>}
      <div className="flex flex-wrap gap-2">
        {runtime.data?.authenticated ? <Button type="button" variant="outline" size="sm" disabled={busy} onClick={() => { void disconnect(); }}>Desconectar Muse Code</Button> : starting || login ? <Button type="button" variant="outline" size="sm" onClick={() => { void cancel(); }}>Cancelar login</Button> : <Button type="button" variant="outline" size="sm" disabled={busy || !runtime.data?.installed} onClick={() => { void connect(); }}>Conectar assinatura Muse Code</Button>}
        {starting && <p role="status" className="self-center text-xs text-muted-foreground">Preparando login…</p>}
        <Button type="button" variant="outline" size="sm" disabled={busy || starting} onClick={() => { void runtime.refresh(); }}><RefreshCw data-icon="inline-start" className={runtime.loading ? "animate-spin motion-reduce:animate-none" : undefined} />Atualizar status e modelos</Button>
        <Button type="button" variant="link" size="sm" onClick={() => { void openUrl("https://dev.meta.ai/docs/muse-code").catch(() => toast.error("Não foi possível abrir a documentação.")); }}><ExternalLink data-icon="inline-start" />Instalar Muse Code</Button>
      </div>
      <Separator />
      <label className="flex cursor-pointer items-center justify-between gap-3 text-xs"><span aria-hidden="true">Disponibilizar Muse Code no Jarvis</span><Switch aria-label="Ativar Muse Code" checked={preferences.enabled} disabled={busy || !runtime.data} onCheckedChange={enabled => { void save({ ...preferences, enabled }); }} /></label>
      <label className="flex cursor-pointer items-center justify-between gap-3 text-xs"><span aria-hidden="true"><span className="block">Limites na barra de status</span><span className="mt-1 block text-muted-foreground">Exibe as cotas da assinatura quando a conta as disponibiliza.</span></span><Switch aria-label="Mostrar limites do Muse Code" checked={preferences.showUsage !== false} disabled={busy || !runtime.data || !preferences.enabled} onCheckedChange={showUsage => { void save({ ...preferences, showUsage }); }} /></label>
      <ProviderUsageWindowSettings
        providerName="Muse Code"
        showFiveHourUsage={preferences.showFiveHourUsage}
        showWeeklyUsage={preferences.showWeeklyUsage}
        disabled={busy || !runtime.data || !preferences.enabled || preferences.showUsage === false}
        onChange={(showFiveHourUsage, showWeeklyUsage) => { void save({ ...preferences, showFiveHourUsage, showWeeklyUsage }); }}
      />
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2"><span className="text-xs font-medium text-muted-foreground">Modelos para seleção</span><Badge variant="secondary">{museModels(runtime.data).length}/{runtime.data?.models.length ?? 0}</Badge></div>
        <p className="text-xs leading-5 text-muted-foreground">Modelos novos ficam disponíveis automaticamente. Ao ocultar um modelo, revise os agentes e fluxos que o utilizam. Conversas em execução continuam normalmente.</p>
        {!runtime.loading && !runtime.data?.installed && <p role="status" className="text-xs text-muted-foreground">Muse Code não encontrado. Instale o CLI para consultar os modelos.</p>}
        {runtime.data?.installed && !runtime.data.authenticated && <p role="status" className="text-xs text-muted-foreground">Use Conectar assinatura Muse Code para entrar neste perfil. Se o login integrado não estiver disponível, atualize o CLI oficial e conecte novamente.</p>}
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

export function MuseProviderDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-xl"><MuseProviderDetails /></DialogContent></Dialog>;
}

export function MuseProviderCard() {
  const runtime = useMuseRuntime();
  const [open, setOpen] = useState(false);
  const enabled = runtime.data?.preferences?.enabled !== false;
  const status = runtime.loading ? "Consultando…" : !enabled ? "Desativado" : runtime.error || runtime.data?.error ? "Verificar configuração" : !runtime.data?.installed ? "Instalação necessária" : !runtime.data.authenticated ? "Login necessário" : "Conectado";
  return <Dialog open={open} onOpenChange={setOpen}>
    <Card size="sm" className="settings-group min-w-0" data-testid="provider-account-muse-code">
      <DialogTrigger render={<CardHeader />} nativeButton={false} aria-label="Detalhes de Muse Code" className="settings-row cursor-pointer transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <span className="flex size-8 shrink-0 items-center justify-center text-muted-foreground"><ProviderIcon kind="muse-code" className="size-5" /></span>
          <div className="flex min-w-0 flex-1 flex-col gap-1"><CardTitle>Muse Code</CardTitle><div className="flex flex-wrap items-center gap-2"><Badge variant="outline">{status}</Badge><CardDescription>CLI local · {museModels(runtime.data).length} modelos ativos</CardDescription></div></div>
          <ChevronRight className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        </div>
      </DialogTrigger>
    </Card>
    <DialogContent className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-xl"><MuseProviderDetails /></DialogContent>
  </Dialog>;
}
