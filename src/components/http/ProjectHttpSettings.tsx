import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save as chooseSavePath } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Download, FolderOpen, Plus, RefreshCw, Save, Trash2, Upload } from "lucide-react";
import { toast } from "sonner";
import { WORKFLOW_COLORS } from "@/components/agents/workflow-appearance";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Input } from "@/components/TextInput";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Hint } from "@/components/ui/hint";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { httpSettingsSchema, type HttpDefaults, type HttpSettings, type HttpVariable } from "@/core/http-client";
import { libraryError } from "@/core/library";

const COLORS = Object.entries(WORKFLOW_COLORS).map(([value, color]) => ({ value, label: color.label }));
const JSON_FILTER = [{ name: "Configuração HTTP do Jarvis", extensions: ["json"] }];

function newVariable(): HttpVariable {
  return { id: crypto.randomUUID(), name: "", value: "", secret: false, enabled: true, configured: false };
}

function Variables({ scope, variables, disabled, onChange }: { scope: string; variables: HttpVariable[]; disabled: boolean; onChange: (variables: HttpVariable[]) => void }) {
  const update = (id: string, patch: Partial<HttpVariable>) => onChange(variables.map(variable => variable.id === id ? { ...variable, ...patch } : variable));
  return <FieldGroup>
    {variables.map((variable, index) => <FieldGroup key={variable.id} className="rounded-md border p-3">
      <FieldGroup className="flex-row flex-wrap items-end gap-3">
        <Field className="min-w-36 flex-1"><FieldLabel htmlFor={`http-var-name-${variable.id}`}>Nome</FieldLabel><Input id={`http-var-name-${variable.id}`} aria-label={`Nome da variável ${index + 1} · ${scope}`} value={variable.name} placeholder="base_url" disabled={disabled} maxLength={128} onChange={event => update(variable.id, { name: event.target.value })} className="font-mono" /></Field>
        <Field className="min-w-44 flex-[2]"><FieldLabel htmlFor={`http-var-value-${variable.id}`}>Valor</FieldLabel><Input id={`http-var-value-${variable.id}`} aria-label={`Valor da variável ${index + 1} · ${scope}`} type={variable.secret ? "password" : "text"} autoComplete="off" value={variable.value} placeholder={variable.secret && variable.configured ? "Segredo salvo · deixe vazio para manter" : "Valor"} disabled={disabled} onChange={event => update(variable.id, { value: event.target.value })} className="font-mono" /></Field>
        <Hint content="Remover variável"><Button type="button" variant="ghost" size="icon" className="cursor-pointer" disabled={disabled} aria-label={`Remover variável ${index + 1} · ${scope}`} onClick={() => onChange(variables.filter(item => item.id !== variable.id))}><Trash2 /></Button></Hint>
      </FieldGroup>
      <FieldGroup className="flex-row flex-wrap gap-4">
        <Field orientation="horizontal" className="w-auto"><Switch id={`http-var-enabled-${variable.id}`} className="cursor-pointer" checked={variable.enabled} disabled={disabled} onCheckedChange={enabled => update(variable.id, { enabled })} aria-labelledby={`http-var-enabled-label-${variable.id}`} /><FieldLabel id={`http-var-enabled-label-${variable.id}`} htmlFor={`http-var-enabled-${variable.id}`}>Ativa<span className="sr-only"> · Variável {index + 1} · {scope}</span></FieldLabel></Field>
        <Field orientation="horizontal" className="w-auto"><Switch id={`http-var-secret-${variable.id}`} className="cursor-pointer" checked={variable.secret} disabled={disabled} onCheckedChange={secret => update(variable.id, { secret, ...(variable.secret && variable.configured ? { value: "", configured: false } : {}) })} aria-labelledby={`http-var-secret-label-${variable.id}`} /><FieldLabel id={`http-var-secret-label-${variable.id}`} htmlFor={`http-var-secret-${variable.id}`}>Secreta<span className="sr-only"> · Variável {index + 1} · {scope}</span></FieldLabel></Field>
        {variable.secret && variable.configured && <Button type="button" size="xs" variant="ghost" className="cursor-pointer" disabled={disabled} onClick={() => update(variable.id, { value: "", configured: false })}>Limpar segredo salvo</Button>}
      </FieldGroup>
    </FieldGroup>)}
    <Button type="button" variant="outline" size="sm" className="w-fit cursor-pointer" disabled={disabled || variables.length >= 200} onClick={() => onChange([...variables, newVariable()])}><Plus data-icon="inline-start" />Adicionar variável · {scope}</Button>
  </FieldGroup>;
}

export function ProjectHttpSettings({ projectId }: { projectId: string }) {
  const [saved, setSaved] = useState<HttpSettings | null>(null);
  const [draft, setDraft] = useState<HttpSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  const [confirmation, setConfirmation] = useState<{ type: "reload" } | { type: "import"; path: string } | null>(null);
  const generation = useRef(0);
  const operation = useRef(false);

  useEffect(() => {
    const current = ++generation.current;
    void invoke<unknown>("get_project_http_settings", { projectId }).then(value => {
      if (generation.current !== current) return;
      const settings = httpSettingsSchema.parse(value);
      if (settings.projectId !== projectId) throw new Error("Mismatched project settings");
      setSaved(settings); setDraft(settings); setError(null);
    }).catch(cause => { if (generation.current === current) setError(libraryError(cause, "Não foi possível carregar o cliente HTTP.")); });
    return () => { generation.current += 1; };
  }, [projectId, reload]);

  const changed = Boolean(saved && draft && JSON.stringify(saved) !== JSON.stringify(draft));
  const patchDefaults = (patch: Partial<HttpDefaults>) => setDraft(current => current ? { ...current, defaults: { ...current.defaults, ...patch } } : current);
  const reloadSettings = () => { setSaved(null); setDraft(null); setError(null); setReload(value => value + 1); };

  async function saveSettings() {
    if (!draft || !changed || operation.current) return;
    operation.current = true; setBusy(true); setError(null);
    const current = generation.current;
    try {
      const settings = httpSettingsSchema.parse(await invoke<unknown>("save_project_http_settings", { projectId, settings: draft }));
      if (generation.current !== current) return;
      setSaved(settings); setDraft(settings); toast.success("Configurações HTTP salvas");
    } catch (cause) {
      if (generation.current === current) setError(libraryError(cause, "Não foi possível salvar. Suas alterações locais foram mantidas."));
    } finally { operation.current = false; if (generation.current === current) setBusy(false); }
  }

  async function transfer(action: "export" | "chooseImport") {
    if (operation.current) return;
    operation.current = true; setBusy(true);
    const current = generation.current;
    try {
      if (action === "export") {
        const path = await chooseSavePath({ title: "Exportar cliente HTTP", defaultPath: "Jarvis-http.json", filters: JSON_FILTER });
        if (!path || generation.current !== current) return;
        await invoke("export_project_http", { projectId, path });
        if (generation.current === current) toast.success("Cliente HTTP exportado sem segredos ou resultados");
      } else {
        const path = await open({ title: "Importar cliente HTTP", multiple: false, directory: false, filters: JSON_FILTER });
        if (typeof path === "string" && generation.current === current) setConfirmation({ type: "import", path });
      }
    } catch (cause) {
      if (generation.current === current) toast.error(libraryError(cause, "Não foi possível acessar a configuração HTTP."));
    } finally { operation.current = false; if (generation.current === current) setBusy(false); }
  }

  async function confirm() {
    if (!confirmation || operation.current) return;
    if (confirmation.type === "reload") { setConfirmation(null); reloadSettings(); return; }
    if (!saved) return;
    operation.current = true; setBusy(true); setError(null);
    const current = generation.current;
    try {
      const settings = httpSettingsSchema.parse(await invoke<unknown>("import_project_http", { projectId, path: confirmation.path, revision: saved.revision }));
      if (generation.current !== current) return;
      setSaved(settings); setDraft(settings); setConfirmation(null); toast.success("Configuração HTTP importada");
    } catch (cause) {
      if (generation.current === current) { setConfirmation(null); setError(libraryError(cause, "Não foi possível importar. A configuração atual foi preservada.")); }
    } finally { operation.current = false; if (generation.current === current) setBusy(false); }
  }

  async function chooseCertificate() {
    if (operation.current) return;
    operation.current = true; setBusy(true);
    const current = generation.current;
    try {
      const path = await open({ title: "Selecionar certificado CA", multiple: false, directory: false, filters: [{ name: "Certificado PEM", extensions: ["pem", "crt"] }] });
      if (typeof path === "string" && generation.current === current) patchDefaults({ caFile: path });
    } catch (cause) { if (generation.current === current) toast.error(libraryError(cause, "Não foi possível selecionar o certificado.")); }
    finally { operation.current = false; if (generation.current === current) setBusy(false); }
  }

  if (!draft || !saved || draft.projectId !== projectId) return error ? <Alert variant="destructive"><AlertTriangle /><AlertTitle>Cliente HTTP indisponível</AlertTitle><AlertDescription>{error}<Button type="button" variant="outline" className="w-fit cursor-pointer" onClick={reloadSettings}><RefreshCw data-icon="inline-start" />Tentar novamente</Button></AlertDescription></Alert> : <CardsSkeleton label="Carregando configurações HTTP" />;

  return <form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); void saveSettings(); }}>
    {error && <Alert variant="destructive"><AlertTriangle /><AlertTitle>Configuração não salva</AlertTitle><AlertDescription>{error}<span>Seu rascunho foi mantido. Para buscar a versão mais recente, recarregue a configuração.</span><Button type="button" variant="outline" className="w-fit cursor-pointer" disabled={busy} onClick={() => setConfirmation({ type: "reload" })}><RefreshCw data-icon="inline-start" />Recarregar configuração</Button></AlertDescription></Alert>}
    <Card><CardHeader><CardTitle>Variáveis compartilhadas</CardTitle><CardDescription>Disponíveis em todos os ambientes deste projeto. Use <code className="font-mono">{"{{nome}}"}</code> nas requisições. Valores do ambiente têm prioridade.</CardDescription></CardHeader><CardContent><Variables scope="Projeto" variables={draft.variables} disabled={busy} onChange={variables => setDraft(current => current ? { ...current, variables } : current)} /><p className="mt-3 text-xs text-muted-foreground">Marque credenciais como secretas, mesmo quando compartilhadas. O valor salvo não é exibido nem incluído na exportação.</p></CardContent></Card>

    <Card><CardHeader><CardTitle>Ambientes</CardTitle><CardDescription>Separe URLs e credenciais de desenvolvimento, homologação e produção. Cada aba HTTP escolhe seu próprio ambiente.</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">
      {!draft.environments.length && <Empty><EmptyHeader><EmptyTitle>Nenhum ambiente</EmptyTitle><EmptyDescription>As requisições já podem usar as variáveis compartilhadas.</EmptyDescription></EmptyHeader></Empty>}
      {draft.environments.map((environment, index) => <Card key={environment.id}><CardHeader><CardTitle>Ambiente {index + 1}</CardTitle></CardHeader><CardContent className="flex flex-col gap-4">
        <FieldGroup className="flex-row flex-wrap items-end gap-3">
          <Field className="min-w-40 flex-1"><FieldLabel htmlFor={`http-env-${environment.id}`}>Nome</FieldLabel><Input id={`http-env-${environment.id}`} aria-label={`Nome do ambiente ${index + 1}`} value={environment.name} maxLength={80} disabled={busy} onChange={event => setDraft(current => current ? { ...current, environments: current.environments.map(item => item.id === environment.id ? { ...item, name: event.target.value } : item) } : current)} /></Field>
          <Field className="w-40"><FieldLabel htmlFor={`http-color-${environment.id}`}>Cor</FieldLabel><Select items={COLORS} value={environment.color} disabled={busy} onValueChange={color => { if (typeof color === "string") setDraft(current => current ? { ...current, environments: current.environments.map(item => item.id === environment.id ? { ...item, color } : item) } : current); }}><SelectTrigger id={`http-color-${environment.id}`} aria-label={`Cor do ambiente ${index + 1}`} className="w-full cursor-pointer"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{COLORS.map(color => <SelectItem key={color.value} value={color.value} className="cursor-pointer"><span className="size-2 shrink-0 rounded-full" style={{ backgroundColor: WORKFLOW_COLORS[color.value as keyof typeof WORKFLOW_COLORS].value }} />{color.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
          <Hint content="Remover ambiente"><Button type="button" variant="ghost" size="icon" className="cursor-pointer" disabled={busy} aria-label={`Remover ambiente ${index + 1}`} onClick={() => setDraft(current => current ? { ...current, environments: current.environments.filter(item => item.id !== environment.id) } : current)}><Trash2 /></Button></Hint>
        </FieldGroup>
        <Variables scope={environment.name || `Ambiente ${index + 1}`} variables={environment.variables} disabled={busy} onChange={variables => setDraft(current => current ? { ...current, environments: current.environments.map(item => item.id === environment.id ? { ...item, variables } : item) } : current)} />
      </CardContent></Card>)}
      <Button type="button" variant="outline" size="sm" className="w-fit cursor-pointer" disabled={busy || draft.environments.length >= 30} onClick={() => setDraft(current => current ? { ...current, environments: [...current.environments, { id: crypto.randomUUID(), name: "Novo ambiente", color: "blue", variables: [] }] } : current)}><Plus data-icon="inline-start" />Adicionar ambiente</Button>
    </CardContent></Card>

    <Card><CardHeader><CardTitle>Execução e histórico</CardTitle><CardDescription>Limites por requisição. Interromper a espera não desfaz o que a API já executou.</CardDescription></CardHeader><CardContent><FieldGroup>
      <FieldGroup className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Field><FieldLabel htmlFor="http-connect">Conexão (segundos)</FieldLabel><Input id="http-connect" type="number" min={1} max={120} required disabled={busy} value={draft.defaults.connectTimeoutSeconds} onChange={event => patchDefaults({ connectTimeoutSeconds: Number(event.target.value) })} /></Field>
        <Field><FieldLabel htmlFor="http-read">Sem receber dados (segundos)</FieldLabel><Input id="http-read" type="number" min={1} max={3600} required disabled={busy} value={draft.defaults.readTimeoutSeconds} onChange={event => patchDefaults({ readTimeoutSeconds: Number(event.target.value) })} /></Field>
        <Field><FieldLabel htmlFor="http-total">Tempo total (segundos)</FieldLabel><Input id="http-total" type="number" min={1} max={86400} disabled={busy} value={draft.defaults.totalTimeoutSeconds ?? ""} placeholder="Sem limite total" onChange={event => patchDefaults({ totalTimeoutSeconds: event.target.value === "" ? null : Number(event.target.value) })} /><FieldDescription>Opcional. Vazio mantém apenas os limites de conexão e inatividade.</FieldDescription></Field>
        <Field><FieldLabel htmlFor="http-history">Resultados no histórico</FieldLabel><Input id="http-history" type="number" min={1} max={1000} required disabled={busy} value={draft.defaults.historyLimit} onChange={event => patchDefaults({ historyLimit: Number(event.target.value) })} /></Field>
        <Field><FieldLabel htmlFor="http-response-size">Resposta armazenada (MiB)</FieldLabel><Input id="http-response-size" type="number" min={1 / 1024} max={100} step="any" required disabled={busy} value={draft.defaults.maxResponseBytes / (1024 * 1024)} onChange={event => patchDefaults({ maxResponseBytes: Math.round(Number(event.target.value) * 1024 * 1024) })} /><FieldDescription>Respostas maiores são identificadas como parciais.</FieldDescription></Field>
      </FieldGroup>
      <Field orientation="horizontal"><FieldLabel htmlFor="http-redirects">Seguir redirecionamentos</FieldLabel><Switch id="http-redirects" checked={draft.defaults.followRedirects} disabled={busy} className="cursor-pointer" onCheckedChange={followRedirects => patchDefaults({ followRedirects })} /></Field>
      <Collapsible><CollapsibleTrigger render={<Button type="button" variant="outline" className="cursor-pointer" />}>Transporte avançado</CollapsibleTrigger><CollapsibleContent className="pt-4"><FieldGroup>
        <Field><FieldLabel htmlFor="http-proxy">Proxy HTTP</FieldLabel><Input id="http-proxy" value={draft.defaults.proxyUrl} disabled={busy} placeholder="http://localhost:8080" onChange={event => patchDefaults({ proxyUrl: event.target.value })} /><FieldDescription>Opcional. Informe a URL sem credenciais.</FieldDescription></Field>
        <Field><FieldLabel htmlFor="http-ca">Certificado CA adicional</FieldLabel><div className="flex gap-2"><Input id="http-ca" value={draft.defaults.caFile} disabled={busy} placeholder="Padrão do sistema" onChange={event => patchDefaults({ caFile: event.target.value })} className="min-w-0 flex-1" /><Hint content="Escolher certificado PEM"><Button type="button" variant="outline" size="icon" className="cursor-pointer" aria-label="Escolher certificado PEM" disabled={busy} onClick={() => { void chooseCertificate(); }}><FolderOpen /></Button></Hint></div></Field>
        <Field orientation="horizontal"><FieldLabel htmlFor="http-tls">Verificar certificado TLS</FieldLabel><Switch id="http-tls" checked={draft.defaults.verifyTls} disabled={busy} className="cursor-pointer" onCheckedChange={verifyTls => patchDefaults({ verifyTls })} /></Field>
        {!draft.defaults.verifyTls && <Alert><AlertTriangle /><AlertTitle>Verificação TLS desativada</AlertTitle><AlertDescription>Use apenas para testes controlados. A identidade do servidor não será validada.</AlertDescription></Alert>}
      </FieldGroup></CollapsibleContent></Collapsible>
    </FieldGroup></CardContent></Card>

    <Card><CardHeader><CardTitle>Exportar e importar</CardTitle><CardDescription>Arquivo JSON versionado com configurações e requisições salvas. Não inclui segredos, resultados nem arquivos enviados.</CardDescription></CardHeader><CardContent className="flex flex-wrap gap-2"><Button type="button" variant="outline" className="cursor-pointer" disabled={busy || changed} onClick={() => { void transfer("export"); }}><Download data-icon="inline-start" />Exportar configuração</Button><Button type="button" variant="outline" className="cursor-pointer" disabled={busy} onClick={() => { void transfer("chooseImport"); }}><Upload data-icon="inline-start" />Importar configuração</Button>{changed && <p className="w-full text-xs text-muted-foreground">Salve suas alterações antes de exportar.</p>}</CardContent></Card>
    <div className="sticky bottom-0 flex justify-end border-t bg-background/95 py-4 backdrop-blur"><Button type="submit" className="cursor-pointer" disabled={busy || !changed}><Save data-icon="inline-start" />{busy ? "Aguarde…" : "Salvar cliente HTTP"}</Button></div>

    <AlertDialog open={confirmation !== null} onOpenChange={visible => { if (!visible && !busy) setConfirmation(null); }}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>{confirmation?.type === "reload" ? "Recarregar configuração?" : "Importar configuração HTTP?"}</AlertDialogTitle><AlertDialogDescription>{confirmation?.type === "reload" ? "As alterações locais não salvas serão descartadas. O Jarvis carregará a versão mais recente do projeto." : "As variáveis, ambientes e opções atuais serão substituídos. As requisições do arquivo serão adicionadas às salvas. Resultados anteriores serão preservados; segredos não vêm no arquivo e precisarão ser configurados."}</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel disabled={busy} className="cursor-pointer">Cancelar</AlertDialogCancel><AlertDialogAction disabled={busy} className="cursor-pointer" onClick={event => { event.preventDefault(); void confirm(); }}>{confirmation?.type === "reload" ? "Recarregar" : "Importar"}</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
  </form>;
}
