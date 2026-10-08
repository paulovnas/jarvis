import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, ArrowLeft, ChevronDown, ChevronRight, FolderOpen, Package, Plus, RefreshCw, ShieldCheck, Store, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { PLUGIN_COMPONENT_LABELS, groupPlugins, pluginCategoryLabel, pluginCatalogSchema, pluginError, pluginReceiptSchema, pluginSourceLabel, type AvailablePlugin, type InstalledPlugin, type PluginCatalog, type PluginOperation, type PluginReceipt } from "@/core/plugins";
import type { ProviderAccount } from "@/core/provider-accounts";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/TextInput";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Spinner } from "@/components/ui/spinner";
import { PluginChangeReview } from "./PluginChangeReview";
import { PluginCreateForm, PluginSourceForm } from "./PluginForms";
import { PluginMcpAuth } from "./PluginMcpAuth";
import { PluginMcpConfiguration } from "./PluginMcpConfiguration";
import { PluginAppsConnection } from "./PluginAppsConnection";
import { PluginCatalogCard } from "./PluginCatalogCard";
import { PluginIcon } from "./PluginIcon";

type Form = { kind: "source" | "create"; revision: number };
type Review = { receipt: PluginReceipt; operation: PluginOperation };

export function PluginsSettings({ accounts = [], onBusyChange }: { accounts?: ProviderAccount[]; onBusyChange?: (busy: boolean) => void }) {
  const [catalog, setCatalog] = useState<PluginCatalog | null>(null);
  const [loading, setLoading] = useState(true); const [error, setError] = useState<string | null>(null);
  const [changing, setBusy] = useState(false); const [authBusy, setAuthBusy] = useState(false);
  const busy = changing || authBusy;
  const [tab, setTab] = useState("store"); const [query, setQuery] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const catalogTop = useRef<HTMLElement>(null);
  const [form, setForm] = useState<Form | null>(null); const [formError, setFormError] = useState<string | null>(null);
  const [review, setReview] = useState<Review | null>(null); const [reviewError, setReviewError] = useState<string | null>(null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const mounted = useRef(false); const pending = useRef(false); const request = useRef(0); const activeReceipt = useRef<string | null>(null);
  const apply = useCallback((raw: unknown) => {
    const next = pluginCatalogSchema.parse(raw);
    if (mounted.current) setCatalog(current => current && current.revision > next.revision ? current : next);
  }, []);
  const refresh = useCallback(async () => {
    const version = ++request.current;
    try { const raw: unknown = await invoke("list_plugins"); if (mounted.current && version === request.current) { apply(raw); setError(null); } }
    catch { if (mounted.current && version === request.current) setError("Não foi possível carregar os plugins."); }
    finally { if (mounted.current && version === request.current) setLoading(false); }
  }, [apply]);
  useEffect(() => {
    let active = true; let stop: (() => void) | undefined;
    mounted.current = true;
    void Promise.resolve().then(() => { if (active) void refresh(); });
    void listen("plugins:changed", () => { if (active) void refresh(); }).then(unlisten => { if (active) stop = unlisten; else unlisten(); }).catch(() => {});
    return () => { active = false; mounted.current = false; request.current += 1; stop?.(); const receiptId = activeReceipt.current; if (receiptId) void invoke("cancel_plugin_change", { receiptId }).catch(() => {}); };
  }, [refresh]);
  useEffect(() => { onBusyChange?.(busy); return () => onBusyChange?.(false); }, [busy, onBusyChange]);

  async function prepare(operation: PluginOperation, expectedRevision = catalog?.revision) {
    if (pending.current || expectedRevision === undefined) return;
    pending.current = true; setBusy(true); setReviewError(null); setFormError(null);
    try {
      const receipt = pluginReceiptSchema.parse(await invoke("preview_plugin_change", { operation, expectedRevision }));
      if (!mounted.current) { void invoke("cancel_plugin_change", { receiptId: receipt.receiptId }).catch(() => {}); return; }
      activeReceipt.current = receipt.receiptId; setReview({ receipt, operation }); setForm(null); setDetailId(null);
    } catch (cause) { if (mounted.current) { const message = pluginError(cause); if (form) setFormError(message); else { setError(message); toast.error(message); } } }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  async function dismissReview() {
    if (pending.current || !review) return;
    activeReceipt.current = null; setReview(null); setReviewError(null);
    try { await invoke("cancel_plugin_change", { receiptId: review.receipt.receiptId }); } catch { /* Native receipts also expire without applying. */ }
  }
  async function confirm() {
    if (pending.current || !review) return;
    pending.current = true; request.current += 1; setBusy(true);
    try {
      apply(await invoke("apply_plugin_change", { receiptId: review.receipt.receiptId, expectedRevision: review.receipt.revision }));
      activeReceipt.current = null;
      if (mounted.current) { setReview(null); setReviewError(null); setError(null); toast.success("Plugins atualizados"); if (["install", "import", "create"].includes(review.operation.action)) { setTab("installed"); setCategory(null); } }
    } catch (cause) { if (mounted.current) setReviewError(pluginError(cause)); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  async function retryReview() {
    if (pending.current || !review) return;
    const operation = review.operation; await dismissReview(); await prepare(operation);
  }
  async function choosePackage(directory: boolean) {
    if (pending.current || !catalog) return;
    pending.current = true; setBusy(true);
    let path: string | null = null;
    try { const selected = await open({ title: directory ? "Importar pasta de plugin" : "Importar pacote de plugin", directory, multiple: false, ...(!directory ? { filters: [{ name: "Pacote de plugin", extensions: ["zip", "tar", "gz", "tgz"] }] } : {}) }); if (typeof selected === "string") path = selected; }
    catch { toast.error("Não foi possível selecionar o plugin."); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
    if (path && mounted.current) await prepare({ action: "import", path });
  }
  async function chooseProject(pluginId: string) {
    if (pending.current || !catalog) return;
    pending.current = true; setBusy(true);
    let path: string | null = null;
    try { const selected = await open({ title: "Ativar plugin em um projeto", directory: true, multiple: false }); if (typeof selected === "string") path = selected; }
    catch { toast.error("Não foi possível selecionar o projeto."); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
    if (path && mounted.current) await prepare({ action: "setEnabled", pluginId, enabled: true, projectPath: path });
  }
  const search = query.trim().toLocaleLowerCase("pt-BR");
  const matches = (plugin: AvailablePlugin | InstalledPlugin) => `${plugin.displayName} ${plugin.name} ${plugin.description} ${plugin.shortDescription ?? ""} ${pluginCategoryLabel(plugin.category)}`.toLocaleLowerCase("pt-BR").includes(search);
  const items = (tab === "store" ? catalog?.available : catalog?.installed)?.filter(matches) ?? [];
  const groups = groupPlugins(items);
  const expandedGroup = tab === "store" && !search ? groups.find(group => group.category === category) : undefined;
  if (category !== null && !expandedGroup) setCategory(null);
  const previewGroups = tab === "store" && !search && !expandedGroup;
  function showCategory(next: string | null) {
    setCategory(next);
    catalogTop.current?.focus();
    catalogTop.current?.scrollIntoView({ block: "start" });
  }
  const selected = catalog?.installed.find(plugin => plugin.id === detailId) ?? catalog?.available.find(plugin => plugin.id === detailId);
  const installedSelected = selected && "components" in selected ? selected : null;
  const eligibleAccounts = accounts.filter(account => account.enabled && account.providerKind === "openai-codex");
  const selectedAccount = eligibleAccounts.find(account => account.alias === catalog?.appsAccountId);

  return <section ref={catalogTop} tabIndex={-1} aria-label="Plugins" className="flex min-w-0 flex-col gap-5">
    <div className="flex flex-wrap items-center justify-between gap-3"><p className="max-w-2xl text-sm text-muted-foreground">Instale pacotes de skills, MCPs, hooks e apps. Revise os recursos e requisitos antes de autorizar cada alteração.</p><div className="flex shrink-0 gap-2"><Button variant="ghost" size="icon-sm" aria-label="Recarregar plugins" className="cursor-pointer" disabled={busy || loading} onClick={() => { setLoading(true); void refresh(); }}><RefreshCw /></Button><DropdownMenu><DropdownMenuTrigger render={<Button size="sm" />} className="cursor-pointer" disabled={!catalog || busy}><Plus data-icon="inline-start" />Adicionar<ChevronDown data-icon="inline-end" /></DropdownMenuTrigger><DropdownMenuContent align="end" className="w-56 max-w-[calc(100vw-2rem)]"><DropdownMenuGroup><DropdownMenuItem className="cursor-pointer whitespace-nowrap" onClick={() => { if (catalog) { setFormError(null); setForm({ kind: "source", revision: catalog.revision }); } }}><Store />Marketplace</DropdownMenuItem><DropdownMenuItem className="cursor-pointer whitespace-nowrap" onClick={() => { void choosePackage(true); }}><FolderOpen />Pasta de plugin</DropdownMenuItem><DropdownMenuItem className="cursor-pointer whitespace-nowrap" onClick={() => { void choosePackage(false); }}><Package />Arquivo de plugin</DropdownMenuItem><DropdownMenuItem className="cursor-pointer whitespace-nowrap" onClick={() => { if (catalog) { setFormError(null); setForm({ kind: "create", revision: catalog.revision }); } }}><Plus />Criar plugin</DropdownMenuItem></DropdownMenuGroup></DropdownMenuContent></DropdownMenu></div></div>
    <Tabs orientation="horizontal" className="min-w-0" value={tab} onValueChange={value => { if (!busy) { setTab(value); setCategory(null); } }}><TabsList aria-label="Catálogo de plugins" aria-orientation="horizontal" className="h-8! flex-row!"><TabsTrigger value="store" className="w-auto! cursor-pointer justify-center!" disabled={busy}><Store data-icon="inline-start" />Loja</TabsTrigger><TabsTrigger value="installed" className="w-auto! cursor-pointer justify-center!" disabled={busy}><Package data-icon="inline-start" />Instalados{catalog && <Badge variant="secondary" className="ml-1">{catalog.installed.length}</Badge>}</TabsTrigger></TabsList></Tabs>
    <Field><FieldLabel htmlFor="plugin-search" className="sr-only">Buscar plugins</FieldLabel><Input id="plugin-search" value={query} onChange={event => { setQuery(event.target.value); setCategory(null); }} placeholder="Buscar plugins…" /></Field>
    {changing && <p role="status" className="flex items-center gap-2 text-xs text-muted-foreground"><Spinner />Preparando alteração dos plugins…</p>}
    {error && <Alert variant="destructive"><AlertTriangle /><AlertTitle>Não foi possível concluir</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>}
    {loading ? <CardsSkeleton label="Carregando plugins" /> : catalog && <>
      {catalog.issues.length > 0 && <Alert><AlertTriangle /><AlertTitle>Origens precisam de atenção</AlertTitle><AlertDescription><ul className="flex list-disc flex-col gap-1 pl-4">{catalog.issues.map((issue, index) => <li key={`${index}:${issue}`}>{issue}</li>)}</ul></AlertDescription></Alert>}
      {expandedGroup && <Button variant="ghost" size="sm" className="cursor-pointer self-start" disabled={busy} onClick={() => showCategory(null)}><ArrowLeft data-icon="inline-start" />Voltar à loja</Button>}
      {!items.length ? <Empty className="border"><EmptyHeader><EmptyTitle>{search ? "Nenhum plugin encontrado" : tab === "store" ? "Sua loja de plugins" : "Nenhum plugin instalado"}</EmptyTitle><EmptyDescription>{search ? "Tente outro nome ou descrição." : tab === "store" ? "Adicione ou atualize um marketplace para explorar os pacotes disponíveis." : "Escolha um pacote na loja ou importe uma pasta ou arquivo."}</EmptyDescription></EmptyHeader></Empty> : <div className="flex min-w-0 flex-col gap-6">{(expandedGroup ? [expandedGroup] : groups).map(group => <section key={group.category} aria-label={group.title} className="flex min-w-0 flex-col gap-3">
        <div className="flex items-center justify-between gap-2"><div className="flex min-w-0 items-center gap-2"><h3 className="text-base font-semibold">{group.title}</h3><Badge variant="outline" className="font-mono">{group.plugins.length}</Badge></div>{previewGroups && group.plugins.length > 6 && <Button variant="ghost" size="sm" className="shrink-0 cursor-pointer" aria-label={`Ver mais em ${group.title}`} disabled={busy} onClick={() => showCategory(group.category)}>Ver mais<ChevronRight data-icon="inline-end" /></Button>}</div>
        <div className="grid min-w-0 grid-cols-[repeat(auto-fill,minmax(min(100%,18rem),1fr))] gap-3">{(previewGroups ? group.plugins.slice(0, 6) : group.plugins).map(plugin => {
          const installed = catalog.installed.find(item => item.id === plugin.id);
          const available = catalog.available.find(item => item.id === plugin.id);
          return <PluginCatalogCard key={plugin.id} plugin={plugin} installed={installed} available={available} marketplaceName={catalog.marketplaces.find(item => item.id === plugin.marketplaceId)?.name} busy={busy} onDetails={() => setDetailId(plugin.id)} onInstall={() => { void prepare({ action: "install", pluginId: plugin.id }); }} onEnabled={enabled => { if (installed) void prepare({ action: "setEnabled", pluginId: installed.id, enabled }); }} />;
        })}</div>
      </section>)}</div>}
      {!expandedGroup && <>
      {tab === "store" && <section aria-label="Marketplaces" className="flex min-w-0 flex-col gap-3"><div className="flex items-center justify-between gap-2"><h3 className="micro-label text-muted-foreground">Marketplaces</h3><Button variant="outline" size="sm" className="cursor-pointer" disabled={busy || !catalog.marketplaces.length} onClick={() => { void prepare({ action: "refreshMarketplace", marketplaceId: null }); }}><RefreshCw data-icon="inline-start" />Atualizar loja</Button></div>{catalog.marketplaces.map(marketplace => <Card key={marketplace.id} size="sm" className="gap-2"><CardHeader className="flex flex-row flex-wrap items-center gap-2"><CardTitle className="min-w-0 flex-1 break-words">{marketplace.name}</CardTitle><Badge variant="outline">{marketplace.builtIn ? "Nativo" : marketplace.refreshed ? "Atualizado" : "Aguardando atualização"}</Badge><Button variant="ghost" size="icon-sm" className="cursor-pointer" aria-label={`Atualizar marketplace ${marketplace.name}`} disabled={busy} onClick={() => { void prepare({ action: "refreshMarketplace", marketplaceId: marketplace.id }); }}><RefreshCw /></Button>{!marketplace.builtIn && <Button variant="ghost" size="icon-sm" className="cursor-pointer text-destructive" aria-label={`Remover marketplace ${marketplace.name}`} disabled={busy} onClick={() => { void prepare({ action: "removeMarketplace", marketplaceId: marketplace.id }); }}><Trash2 /></Button>}</CardHeader><CardContent><p className="font-mono text-xs wrap-anywhere text-muted-foreground">{marketplace.source}{marketplace.refName ? ` · ${marketplace.refName}` : ""}</p>{marketplace.sparsePaths.length > 0 && <p className="mt-1 font-mono text-xs wrap-anywhere text-muted-foreground">{marketplace.sparsePaths.join(" · ")}</p>}</CardContent></Card>)}</section>}
      <Card size="sm"><CardHeader><CardTitle>Conta para Apps</CardTitle><CardDescription className="text-xs">Apps hospedados exigem uma conta ChatGPT conectada, acesso ao serviço e autorização do app. Instalar o pacote não conecta o serviço.</CardDescription></CardHeader><CardContent><Field><FieldLabel htmlFor="plugin-apps-account">Conta ChatGPT</FieldLabel><Select value={catalog.appsAccountId ?? "none"} onValueChange={value => { if (value) void prepare({ action: "setAppsAccount", accountId: value === "none" ? null : value }); }} disabled={busy}><SelectTrigger id="plugin-apps-account" className="w-full cursor-pointer"><SelectValue>{selectedAccount?.alias ?? (catalog.appsAccountId ? `${catalog.appsAccountId} · indisponível` : "Nenhuma conta")}</SelectValue></SelectTrigger><SelectContent><SelectGroup><SelectItem value="none" className="cursor-pointer">Nenhuma conta</SelectItem>{eligibleAccounts.map(account => <SelectItem key={account.alias} value={account.alias} className="cursor-pointer">{account.alias}</SelectItem>)}</SelectGroup></SelectContent></Select><FieldDescription>{eligibleAccounts.length ? "A conta selecionada será usada pelos apps dos plugins." : "Conecte e ative uma conta ChatGPT em Provedores para usar apps hospedados."}</FieldDescription></Field></CardContent></Card>
      </>}
    </>}
    <Dialog open={form !== null} onOpenChange={value => { if (!value && !pending.current) setForm(null); }}><DialogContent className="dark flex max-h-[85dvh] flex-col overflow-hidden sm:max-w-2xl"><DialogHeader className="shrink-0 pr-6"><DialogTitle>{form?.kind === "source" ? "Adicionar marketplace" : "Criar plugin"}</DialogTitle><DialogDescription>{form?.kind === "source" ? "Escolha uma origem para explorar os plugins disponíveis." : "Organize instruções e integrações em um pacote reutilizável."}</DialogDescription></DialogHeader>{form?.kind === "source" ? <PluginSourceForm busy={busy} error={formError} onSubmit={operation => { void prepare(operation, form.revision); }} onCancel={() => setForm(null)} /> : form && <PluginCreateForm busy={busy} error={formError} onSubmit={operation => { void prepare(operation, form.revision); }} onCancel={() => setForm(null)} />}</DialogContent></Dialog>
    <Dialog open={selected !== undefined && selected !== null} onOpenChange={value => { if (!value && !busy && !pending.current) setDetailId(null); }}><DialogContent className="dark flex max-h-[85dvh] flex-col overflow-hidden sm:max-w-2xl"><DialogHeader className="shrink-0 pr-6"><div className="flex items-start gap-3">{selected && <PluginIcon name={selected.displayName} src={selected.iconDataUrl} large />}<div className="flex min-w-0 flex-1 flex-col gap-2"><DialogTitle className="break-words">{selected?.displayName}</DialogTitle>{selected?.category && <p className="micro-label text-muted-foreground">{pluginCategoryLabel(selected.category)}</p>}<DialogDescription>{selected?.description || "Confira os componentes e requisitos deste plugin."}</DialogDescription></div></div></DialogHeader>{selected && <div className="flex min-h-0 flex-col gap-4 overflow-y-auto px-1 pb-1">
      {installedSelected ? <><dl className="grid min-w-0 gap-1 text-xs"><dt className="micro-label text-muted-foreground">Pacote instalado</dt><dd className="font-mono wrap-anywhere">{installedSelected.rootPath}</dd><dt className="micro-label mt-2 text-muted-foreground">Dados persistentes</dt><dd className="font-mono wrap-anywhere">{installedSelected.dataPath}</dd></dl>{!installedSelected.integrityValid && <Alert variant="destructive"><AlertTriangle /><AlertTitle>Pacote alterado fora do Jarvis</AlertTitle><AlertDescription>Os recursos foram suspensos. Atualize ou importe o pacote novamente para revisar o conteúdo.</AlertDescription></Alert>}{installedSelected.warnings.map((warning, index) => <p key={`${index}:${warning}`} className="text-xs text-muted-foreground">{warning}</p>)}<section aria-label="Componentes instalados" className="flex flex-col gap-2">{installedSelected.components.map(component => <Card key={component.id} size="sm" className="gap-2"><CardHeader className="flex flex-row items-center gap-2"><CardTitle className="min-w-0 flex-1 break-words">{component.name}</CardTitle><Badge variant="outline">{PLUGIN_COMPONENT_LABELS[component.kind]}</Badge><Switch className="cursor-pointer" aria-label={`Ativar componente ${component.name}`} checked={component.enabled && component.supported} disabled={busy || !component.supported || !installedSelected.integrityValid} onCheckedChange={enabled => { void prepare({ action: "configureComponent", pluginId: installedSelected.id, componentId: component.id, enabled }); }} /></CardHeader><CardContent><p className="text-xs whitespace-pre-wrap wrap-anywhere text-muted-foreground">{component.detail}</p>{!component.supported && <p className="mt-2 text-xs text-muted-foreground">Este recurso precisa de integração adicional para funcionar no Jarvis.</p>}{component.kind === "apps" && <p className="mt-2 text-xs text-muted-foreground">{!catalog?.appsAccountId ? "Selecione uma conta ChatGPT para usar este app." : !selectedAccount ? "A conta selecionada está indisponível. Escolha outra conta ChatGPT." : `Conta ChatGPT: ${selectedAccount.alias}. O acesso ao serviço e a autorização do conector serão verificados ao usar o app.`}</p>}{component.kind === "apps" && component.supported && <PluginAppsConnection key={`${installedSelected.hash}:${component.id}:${catalog?.appsAccountId ?? "none"}`} serverId={component.mcpServerId ?? null} connectUrl={component.appConnectUrl ?? null} name={component.name} active={component.enabled && installedSelected.enabled && installedSelected.integrityValid && selectedAccount !== undefined} disabled={busy} onBusyChange={setAuthBusy} />}{component.kind === "hooks" && !component.trusted && <p className="mt-2 text-xs text-muted-foreground">Comandos ainda não autorizados. Instalar ou ativar o componente não autoriza os hooks.</p>}{component.kind === "mcp" && component.mcpServerId && <PluginMcpConfiguration key={`${installedSelected.hash}:${component.id}:configuration`} serverId={component.mcpServerId} name={component.name} active={component.enabled && installedSelected.enabled && installedSelected.integrityValid && component.supported} disabled={busy} onBusyChange={setAuthBusy} />}{component.kind === "mcp" && component.mcpOAuth && component.mcpServerId && <PluginMcpAuth key={`${installedSelected.hash}:${component.id}:oauth`} serverId={component.mcpServerId} name={component.name} disabled={busy || !component.enabled || !installedSelected.enabled || !installedSelected.integrityValid} onBusyChange={setAuthBusy} />}</CardContent></Card>)}</section>{installedSelected.components.some(component => component.kind === "hooks") && <Button variant="outline" className="cursor-pointer self-start" disabled={busy || !installedSelected.integrityValid} onClick={() => { void prepare({ action: "trustHooks", pluginId: installedSelected.id, trusted: !installedSelected.components.filter(component => component.kind === "hooks").every(component => component.trusted) }); }}><ShieldCheck data-icon="inline-start" />{installedSelected.components.filter(component => component.kind === "hooks").every(component => component.trusted) ? "Revogar autorização dos hooks" : "Revisar e autorizar hooks"}</Button>}<Button variant="outline" className="cursor-pointer self-start" disabled={busy || !installedSelected.integrityValid} onClick={() => { void chooseProject(installedSelected.id); }}><FolderOpen data-icon="inline-start" />Ativar em um projeto</Button>{Object.keys(installedSelected.projectOverrides).length > 0 && <section aria-label="Ativação por projeto" className="flex flex-col gap-2"><h3 className="micro-label text-muted-foreground">Ativação por projeto</h3>{Object.entries(installedSelected.projectOverrides).map(([projectPath, enabled]) => <div key={projectPath} className="flex items-center gap-3"><span className="min-w-0 flex-1 font-mono text-xs wrap-anywhere">{projectPath}</span><Switch className="cursor-pointer" aria-label={`Ativar plugin em ${projectPath}`} checked={enabled} disabled={busy} onCheckedChange={active => { void prepare({ action: "setEnabled", pluginId: installedSelected.id, enabled: active, projectPath }); }} /></div>)}</section>}</> : "source" in selected && <><p className="font-mono text-xs wrap-anywhere text-muted-foreground">{pluginSourceLabel(selected.source)}</p>{selected.requirements.length > 0 && <section aria-label="Requisitos"><h3 className="micro-label text-muted-foreground">Requisitos</h3><ul className="mt-2 flex list-disc flex-col gap-1 pl-4 text-sm text-muted-foreground">{selected.requirements.map((requirement, index) => <li key={`${index}:${requirement}`}>{requirement}</li>)}</ul></section>}<p className="text-xs text-muted-foreground">Os componentes e comandos serão apresentados na revisão de instalação.</p></>}
    </div>}<DialogFooter className="shrink-0"><Button variant="ghost" className="cursor-pointer" disabled={busy} onClick={() => setDetailId(null)}>Fechar</Button>{installedSelected ? <><Button variant="destructive" className="cursor-pointer" disabled={busy} onClick={() => { void prepare({ action: "uninstall", pluginId: installedSelected.id }); }}>Desinstalar</Button><Button className="cursor-pointer" disabled={busy} onClick={() => { void prepare({ action: "update", pluginId: installedSelected.id }); }}>Revisar atualização</Button></> : selected && "installable" in selected && <Button className="cursor-pointer" disabled={busy || !selected.installable} onClick={() => { void prepare({ action: "install", pluginId: selected.id }); }}>Revisar instalação</Button>}</DialogFooter></DialogContent></Dialog>
    <Dialog open={review !== null} onOpenChange={(value, details) => { if (!value && details.reason !== "outside-press" && details.reason !== "escape-key") void dismissReview(); }}><DialogContent className="dark flex max-h-[90dvh] flex-col overflow-hidden sm:max-w-3xl"><DialogHeader className="shrink-0 pr-6"><DialogTitle>{review?.receipt.preview.title ?? "Revisar alteração"}</DialogTitle><DialogDescription>A alteração só será aplicada depois da sua confirmação.</DialogDescription></DialogHeader>{review && <div className="min-h-0 overflow-y-auto px-1 pb-1"><PluginChangeReview preview={review.receipt.preview} /></div>}{reviewError && <p role="alert" className="text-sm text-destructive">{reviewError}</p>}<DialogFooter className="shrink-0"><Button variant="ghost" className="cursor-pointer" disabled={busy} onClick={() => { void dismissReview(); }}>Cancelar</Button>{reviewError ? <Button className="cursor-pointer" disabled={busy} onClick={() => { void retryReview(); }}>Revisar novamente</Button> : <Button className="cursor-pointer" disabled={busy} onClick={() => { void confirm(); }}>{busy && <Spinner data-icon="inline-start" />}Confirmar alteração</Button>}</DialogFooter></DialogContent></Dialog>
  </section>;
}
