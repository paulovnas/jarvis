import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Copy, ExternalLink, FolderOpen, Globe, PlugZap, RefreshCw, Unplug } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { browserError, browserExtensionSetupSchema } from "@/core/browser";
import { writeClipboardText } from "@/core/clipboard";
import { browserApplications, browserPreferencesSchema, systemSnapshotSchema, type BrowserPreferences } from "@/core/system-preferences";
import { useBrowserExtensionStatus } from "@/hooks/use-browser-extension-status";

const extensionPages = { chrome: "chrome://extensions", edge: "edge://extensions", brave: "brave://extensions", chromium: "chrome://extensions", firefox: "about:debugging#/runtime/this-firefox" } as const;

export function BrowserSettings() {
  const [preferences, setPreferences] = useState<BrowserPreferences | null>(null);
  const [setup, setSetup] = useState<{ path: string; connectionCode: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const extension = useBrowserExtensionStatus(preferences?.mode === "extension");
  useEffect(() => {
    let alive = true;
    let changed = false;
    const subscription = listen("system:changed", event => {
      const next = systemSnapshotSchema.safeParse(event.payload);
      if (alive && next.success) { changed = true; setPreferences(next.data.preferences.browser); }
    });
    void subscription.catch(cause => { if (alive) setError(browserError(cause)); });
    void invoke("get_system_preferences").then(value => {
      const next = systemSnapshotSchema.parse(value);
      if (alive && !changed) setPreferences(next.preferences.browser);
    }).catch(cause => { if (alive) setError(browserError(cause)); });
    return () => { alive = false; void subscription.then(stop => stop()).catch(() => {}); };
  }, [attempt]);

  const run = async (action: () => Promise<void>) => {
    if (busy) return;
    setBusy(true); setError(null);
    try { await action(); } catch (cause) { setError(browserError(cause)); } finally { setBusy(false); }
  };
  const save = (patch: Partial<BrowserPreferences>) => run(async () => {
    const current = systemSnapshotSchema.parse(await invoke("get_system_preferences"));
    const browser = browserPreferencesSchema.parse({ ...current.preferences.browser, ...patch });
    const saved = systemSnapshotSchema.parse(await invoke("save_system_preferences", { preferences: { ...current.preferences, browser } }));
    if (browser.application !== current.preferences.browser.application) setSetup(null);
    setPreferences(saved.preferences.browser);
    toast.success("Preferências do navegador salvas");
  });
  const copy = (value: string, message: string) => run(async () => { await writeClipboardText(value); toast.success(message); });

  if (!preferences) return error
    ? <div className="space-y-3"><p role="alert" className="text-xs text-destructive">{error}</p><Button variant="outline" className="cursor-pointer" onClick={() => { setError(null); setAttempt(value => value + 1); }}>Tentar novamente</Button></div>
    : <div role="status" aria-label="Carregando preferências do navegador" className="space-y-4"><Skeleton className="h-40" /><Skeleton className="h-64" /></div>;

  const connected = extension.status?.state === "connected";
  const firefox = preferences.application === "firefox";
  const problem = error ?? extension.error ?? extension.status?.error;
  return <div className="space-y-4">
    <Card className="gap-4 p-5">
      <div><h3 className="flex items-center gap-2 text-sm font-medium"><Globe className="size-4 text-onedark-cyan" />Navegação dos agentes</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">Escolha onde abrir novas abas. As abas já vinculadas continuam no navegador de origem.</p></div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2"><Label htmlFor="browser-mode" className="text-xs">Modo de navegação</Label><Select value={preferences.mode} disabled={busy} onValueChange={mode => { if (mode === "embedded" || mode === "extension") void save({ mode }); }}><SelectTrigger id="browser-mode" className="w-full cursor-pointer text-xs"><SelectValue>{preferences.mode === "embedded" ? "Embutido no Jarvis" : "Extensão do navegador"}</SelectValue></SelectTrigger><SelectContent><SelectItem value="embedded" className="cursor-pointer">Embutido no Jarvis</SelectItem><SelectItem value="extension" className="cursor-pointer">Extensão do navegador</SelectItem></SelectContent></Select></div>
        {preferences.mode === "extension" && <div className="space-y-2"><Label htmlFor="browser-application" className="text-xs">Navegador instalado</Label><Select value={preferences.application} disabled={busy} onValueChange={application => { if (application && application in browserApplications) void save({ application: application as BrowserPreferences["application"] }); }}><SelectTrigger id="browser-application" className="w-full cursor-pointer text-xs"><SelectValue>{browserApplications[preferences.application]}</SelectValue></SelectTrigger><SelectContent>{Object.entries(browserApplications).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer">{label}</SelectItem>)}</SelectContent></Select></div>}
      </div>
      <p className="text-xs leading-5 text-muted-foreground">{preferences.mode === "embedded" ? "Navegue em uma área integrada ao chat, com uma sessão separada do seu navegador pessoal." : "Use a sessão do seu navegador para interagir com páginas, capturar imagens e inspecionar console e rede. A extensão controla apenas as abas abertas ou vinculadas ao chat."}</p>
    </Card>
    {preferences.mode === "extension" && <>
      <Card className="gap-3 p-5">
        <div className="flex flex-wrap items-center justify-between gap-3"><h3 className="flex items-center gap-2 text-sm font-medium"><PlugZap className="size-4 text-onedark-cyan" />Conexão local</h3><Badge variant="outline" className={connected ? "text-onedark-green" : "text-muted-foreground"}>{connected ? "Conectada" : extension.status?.state === "error" ? "Falha na conexão" : "Aguardando extensão"}</Badge></div>
        <p className="text-xs leading-5 text-muted-foreground">{connected ? `${extension.status?.profileLabel ?? "Navegador conectado"}${extension.status?.extensionVersion ? ` · extensão ${extension.status.extensionVersion}` : ""}` : "Mantenha o navegador aberto. Após a instalação, a reconexão acontece automaticamente."}</p>
        <p className="text-xs leading-5 text-muted-foreground">Um perfil de navegador por vez. Para trocar o navegador já pareado, revogue a conexão e gere um novo código.</p>
        <div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" disabled={busy} className="cursor-pointer" onClick={() => void run(async () => { await invoke("open_browser_application", { application: preferences.application }); })}><ExternalLink className="size-3.5" />Abrir {browserApplications[preferences.application]}</Button><Hint content="Atualizar estado da conexão"><Button variant="ghost" size="icon-sm" aria-label="Atualizar conexão" className="cursor-pointer" onClick={extension.refresh}><RefreshCw className="size-3.5" /></Button></Hint><Button variant="ghost" size="sm" disabled={busy} className="cursor-pointer text-muted-foreground" onClick={() => void run(async () => { await invoke("revoke_browser_extension"); setSetup(null); extension.refresh(); toast.success("Conexão revogada. Gere um novo código para conectar novamente."); })}><Unplug className="size-3.5" />Revogar conexão</Button></div>
      </Card>
      <Card className="gap-4 p-5">
        <div><h3 className="text-sm font-medium">Instalar extensão externa</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{firefox ? "Carregue a extensão temporariamente no Firefox, sem publicar na loja. Ela precisa ser carregada novamente ao reiniciar o navegador. Uma instalação permanente exige um pacote assinado pela Mozilla." : "A instalação é feita pela pasta da extensão, sem a loja do navegador."} Refaça a preparação e recarregue a extensão após atualizar o Jarvis.</p></div>
        <ol className="space-y-4 text-xs leading-5">
          <li><p className="font-medium">1. Prepare a extensão</p><p className="text-muted-foreground">O Jarvis salva os arquivos em uma pasta fixa deste computador.</p><Button size="sm" variant="secondary" disabled={busy} className="mt-2 cursor-pointer" onClick={() => void run(async () => { setSetup(browserExtensionSetupSchema.parse(await invoke("prepare_browser_extension"))); })}>{busy ? "Aguarde…" : "Preparar extensão"}</Button>{setup && <div className="mt-2 space-y-2"><p className="break-all rounded-md border border-border bg-sidebar px-3 py-2 font-mono text-[11px]">{setup.path}</p><div className="flex flex-wrap gap-2"><Button size="sm" variant="outline" disabled={busy} className="cursor-pointer" onClick={() => void run(async () => { await invoke("open_browser_extension_directory"); })}><FolderOpen className="size-3.5" />Abrir pasta</Button><Button size="sm" variant="ghost" disabled={busy} className="cursor-pointer" onClick={() => void copy(setup.path, "Caminho copiado")}><Copy className="size-3.5" />Copiar caminho</Button></div></div>}</li>
          <li><p className="font-medium">2. Carregue no navegador</p><p className="text-muted-foreground">Acesse <span className="font-mono break-all">{extensionPages[preferences.application]}</span>{firefox ? <>, clique em <strong>Carregar extensão temporária</strong> e selecione o arquivo <span className="font-mono">manifest.json</span> da pasta acima. Se solicitado, permita o acesso aos sites nas permissões da extensão.</> : <>, ative <strong>Modo do desenvolvedor</strong>, clique em <strong>Carregar sem compactação</strong> e selecione a pasta acima.</>}</p><Button size="sm" variant="ghost" disabled={busy} className="mt-1 cursor-pointer" onClick={() => void copy(extensionPages[preferences.application], "Endereço das extensões copiado")}><Copy className="size-3.5" />Copiar endereço</Button></li>
          <li><p className="font-medium">3. Conecte ao Jarvis</p><p className="text-muted-foreground">Abra as opções da extensão Jarvis, cole o código e conecte. O código autoriza este navegador; mantenha-o privado.</p><Button size="sm" variant="secondary" disabled={busy || !setup} className="mt-2 cursor-pointer" onClick={() => { if (setup) void copy(setup.connectionCode, "Código de conexão copiado"); }}><Copy className="size-3.5" />Copiar código de conexão</Button></li>
        </ol>
      </Card>
    </>}
    {problem && <p role="alert" className="text-xs text-destructive">{problem}</p>}
  </div>;
}
