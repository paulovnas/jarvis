import { useId, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ChevronRight, ExternalLink, Plus } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Switch } from "@/components/ui/switch";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Choice, Field, FieldHelp } from "./CustomProviderFields";
import { CustomModelEditor } from "./CustomModelEditor";
import { CustomModelDiscovery } from "./CustomModelDiscovery";
import { normalizeCustomBaseUrl, normalizeOllamaBaseUrl, protocolLabels, supportsModelLookup, validateCustomProvider, type CustomConfig, type CustomModel } from "@/core/custom-provider";
import { validateProviderDisplayName, type ProviderAccount } from "@/core/provider-accounts";
import { ProviderDisplayNameField } from "./ProviderDisplayNameField";
import { Hint } from "@/components/ui/hint";

function emptyModel(): CustomModel {
  return { id: "", name: "", contextWindow: 0, maxOutputTokens: 0, supportsChat: true, supportsImageGeneration: false, supportsImages: false, supportsTools: true, reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null, thinkingBudget: null };
}
export function CustomProviderForm({ account, providerKind = account?.providerKind ?? "custom", onSaved, onCancel, onBusyChange }: { account?: ProviderAccount; providerKind?: string; onSaved: (account: ProviderAccount) => void; onCancel: () => void; onBusyChange?: (busy: boolean) => void }) {
  const balanceKeyId = useId();
  const nativeRouter = providerKind === "openrouter";
  const nativeOllama = providerKind === "ollama";
  const [alias, setAlias] = useState(account?.alias ?? (nativeRouter ? "openrouter-pessoal" : nativeOllama ? "ollama-pessoal" : ""));
  const [apiKey, setApiKey] = useState("");
  const [balanceKey, setBalanceKey] = useState("");
  const [removeBalanceKey, setRemoveBalanceKey] = useState(false);
  const [displayName, setDisplayName] = useState<string | null>(account?.displayName ?? (nativeRouter ? "OpenRouter" : nativeOllama ? "Ollama" : null));
  const [config, setConfig] = useState<CustomConfig>(account?.custom ?? { baseUrl: nativeRouter ? "https://openrouter.ai/api/v1" : nativeOllama ? "http://localhost:11434" : "", protocol: nativeOllama ? "ollama" : "openai-completions", authMode: "bearer", tokenField: "max_tokens", replayUnsignedThinking: false, models: [emptyModel()] });
  const [modelKeys, setModelKeys] = useState(() => config.models.map((_, index) => index));
  const nextKey = useRef(modelKeys.length);
  const [expandedModel, setExpandedModel] = useState<number | null>(account ? null : 0);
  const [pending, setPending] = useState<Set<number>>(() => new Set());
  const [advanced, setAdvanced] = useState(false);
  const [saving, setSaving] = useState(false);
  const [discovering, setDiscovering] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lock = useRef(false);
  const markPending = (key: number, busy: boolean) => setPending(current => {
    if (current.has(key) === busy) return current;
    const next = new Set(current);
    if (busy) next.add(key); else next.delete(key);
    return next;
  });
  async function save(event: FormEvent) {
    event.preventDefault();
    if (lock.current || pending.size > 0 || discovering) return;
    const nextConfig = { ...config, baseUrl: nativeOllama ? normalizeOllamaBaseUrl(config.baseUrl) : account?.custom?.baseUrl === config.baseUrl ? config.baseUrl : normalizeCustomBaseUrl(config.baseUrl) };
    const error = validateCustomProvider(alias, nextConfig, apiKey, !!account);
    if (error) {
      setError(error);
      const invalidIndex = config.models.findIndex(model => validateCustomProvider(alias, { ...config, models: [model] }, apiKey, !!account) !== null);
      if (invalidIndex >= 0) setExpandedModel(modelKeys[invalidIndex]);
      return;
    }
    const name = displayName ?? alias;
    if (!account || displayName !== null) {
      const nameError = validateProviderDisplayName(name);
      if (nameError) { setError(nameError); return; }
    }
    lock.current = true; setSaving(true); setError(null); onBusyChange?.(true);
    try {
      const result = await invoke<ProviderAccount>(nativeOllama ? "save_ollama_provider" : nativeRouter ? "save_openrouter_provider" : "save_custom_provider", { alias, ...(!account || displayName !== null ? { displayName: name.trim() } : {}), config: nextConfig, ...(!nativeOllama ? { apiKey: apiKey || null } : {}), editing: !!account, ...(nativeRouter ? { balanceKey: balanceKey || null, removeBalanceKey } : {}) });
      setApiKey(""); setBalanceKey(""); toast.success("Provedor salvo"); onSaved(result);
    } catch (error) {
      setError(typeof error === "object" && error !== null && "message" in error && typeof error.message === "string" ? error.message : "Não foi possível salvar o provedor.");
    } finally { lock.current = false; setSaving(false); onBusyChange?.(false); }
  }
  return <TooltipProvider delay={150}><form onSubmit={event => void save(event)} className="flex min-w-0 flex-col gap-4" aria-busy={saving}>
    <fieldset disabled={saving} className="flex min-w-0 flex-col gap-4">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Alias completo" help="Nome desta conexão no Jarvis, escolhido por você. Use letras, números e hífens. Depois de salvar, o alias não pode ser alterado." value={alias} disabled={!!account} maxLength={64} placeholder="meu-provedor" autoComplete="off" onChange={e => setAlias(e.target.value)} />
      {!nativeRouter && !nativeOllama && <>
        <Choice label="Endpoint" help="Formato da API oferecido pelo provedor. Confira a documentação dele: Chat Completions, Responses ou Anthropic Messages. No OpenRouter, Chat Completions é a opção mais comum." value={config.protocol} items={Object.entries(protocolLabels).filter(([value]) => value !== "ollama").map(([value, label]) => ({ value: value as CustomConfig["protocol"], label }))} onChange={protocol => setConfig(c => ({ ...c, protocol, authMode: "bearer", replayUnsignedThinking: protocol === "anthropic-messages", models: c.models.map(m => ({ ...m, reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null, thinkingBudget: null })) }))} disabled={saving} />
      </>}
      </div>
      <ProviderDisplayNameField value={displayName ?? alias} onChange={setDisplayName} disabled={saving} required={!account} />
      {!nativeRouter && <>
      <Field label="URL base" help={nativeOllama ? "Endereço do servidor Ollama, sem /v1 ou /api. Use HTTP apenas neste computador ou em uma rede privada; servidores públicos precisam de HTTPS." : "Endereço da API informado pelo provedor. Novos endereços sem caminho recebem /v1; caminhos personalizados são preservados. Exemplo: https://openrouter.ai/api/v1. Não inclua a chave na URL."} value={config.baseUrl} placeholder={nativeOllama ? "http://localhost:11434" : "https://seu-provedor.com/v1"} autoComplete="off" onChange={e => setConfig(c => ({ ...c, baseUrl: e.target.value }))} onBlur={() => setConfig(c => ({ ...c, baseUrl: nativeOllama ? normalizeOllamaBaseUrl(c.baseUrl) : account?.custom?.baseUrl === c.baseUrl ? c.baseUrl : normalizeCustomBaseUrl(c.baseUrl) }))} />
      </>}
      {!nativeOllama && <Field label="Chave de API" help="Crie uma chave no painel do seu provedor. Ela fica protegida no cofre de credenciais do sistema; ao editar, deixe vazio para manter a chave atual." type="password" value={apiKey} autoComplete="new-password" placeholder={account ? "Manter chave atual" : "Cole sua chave"} onChange={e => setApiKey(e.target.value)} />}
      {nativeRouter && <div className="space-y-3"><Field label="Chave de gerenciamento (opcional)" help="Usada apenas para consultar o saldo em dólares. Use uma chave de gerenciamento da mesma conta; ela fica no cofre e nunca é usada para gerar respostas. Ao editar, deixe vazio para manter a atual. Ao trocar a chave de API, informe novamente esta chave." type="password" value={balanceKey} autoComplete="new-password" disabled={removeBalanceKey} onChange={event => setBalanceKey(event.target.value)} />{account && <div className="flex items-center gap-2"><Switch id={balanceKeyId} checked={removeBalanceKey} onCheckedChange={setRemoveBalanceKey} className="cursor-pointer" /><Label htmlFor={balanceKeyId} className="cursor-pointer text-xs">Remover chave de gerenciamento salva</Label></div>}</div>}
      {!nativeRouter && !nativeOllama && config.protocol !== "openai-responses" && <Collapsible open={advanced} onOpenChange={setAdvanced}>
        <CollapsibleTrigger render={<Button type="button" variant="ghost" size="sm" />} className="h-7 w-full cursor-pointer justify-start px-0 text-xs text-muted-foreground">Compatibilidade avançada<ChevronRight className={`ml-auto size-3.5 transition-transform ${advanced ? "rotate-90" : ""}`} /></CollapsibleTrigger>
        <CollapsibleContent className="space-y-3 pt-3">
          {config.protocol === "anthropic-messages" ? <Choice label="Autenticação" help="OpenRouter e gateways costumam usar Authorization: Bearer. A API direta da Anthropic usa x-api-key. Consulte o exemplo de requisição na documentação do provedor." value={config.authMode} items={[{ value: "bearer", label: "Bearer · gateways / OpenRouter" }, { value: "x-api-key", label: "x-api-key · Anthropic" }]} onChange={authMode => setConfig(c => ({ ...c, authMode, replayUnsignedThinking: authMode === "bearer" }))} disabled={saving} /> : <Choice label="Campo de limite de saída" help="Nome do parâmetro que limita a resposta. OpenRouter usa max_tokens; alguns modelos OpenAI exigem max_completion_tokens. Só altere se a documentação do endpoint pedir." value={config.tokenField} items={[{ value: "max_tokens", label: "max_tokens" }, { value: "max_completion_tokens", label: "max_completion_tokens" }]} onChange={tokenField => setConfig(c => ({ ...c, tokenField }))} disabled={saving} />}
          {config.protocol === "anthropic-messages" && <div className="flex items-center gap-1"><label className="flex cursor-pointer items-center gap-2 text-xs"><Switch checked={config.replayUnsignedThinking} disabled={saving} onCheckedChange={replayUnsignedThinking => setConfig(c => ({ ...c, replayUnsignedThinking }))} className="cursor-pointer" aria-label="Reenviar thinking sem assinatura" />Reenviar thinking sem assinatura</label><FieldHelp label="Reenviar thinking sem assinatura">Alguns gateways precisam receber novamente o raciocínio entre chamadas de ferramentas. Use apenas se o gateway aceitar blocos sem assinatura; na API direta Anthropic, mantenha desativado.</FieldHelp></div>}
        </CollapsibleContent>
      </Collapsible>}
      <div className="flex flex-wrap items-center justify-between gap-2"><h3 className="settings-section-title text-muted-foreground">Modelos</h3><div className="flex items-center gap-1">{!nativeOllama && supportsModelLookup(config.baseUrl) && <Hint content="Abrir catálogo OpenRouter"><Button type="button" variant="ghost" size="icon-sm" aria-label="Abrir catálogo OpenRouter" onClick={() => { void openUrl("https://openrouter.ai/models").catch(() => toast.error("Não foi possível abrir o catálogo.")); }}><ExternalLink data-icon="inline-start" /></Button></Hint>}<Button type="button" variant="outline" size="sm" disabled={saving || config.models.length >= 100} onClick={() => { const key = nextKey.current++; setModelKeys(keys => [...keys, key]); setConfig(c => ({ ...c, models: [...c.models, emptyModel()] })); setExpandedModel(key); }}><Plus data-icon="inline-start" />Adicionar modelo</Button></div></div>
      {nativeOllama && <p className="text-xs text-muted-foreground">Cadastre apenas os modelos que deseja usar pelo ID exato, incluindo a tag. O Jarvis consulta esse modelo no servidor; outros modelos instalados não são adicionados.</p>}
      {!nativeOllama && <CustomModelDiscovery config={config} apiKey={apiKey} alias={account?.alias} disabled={saving || pending.size > 0} onPendingChange={setDiscovering} onImport={models => {
        const retained = config.models.map((model, index) => ({ model, key: modelKeys[index] })).filter(({ model }) => model.id || model.name || model.contextWindow || model.maxOutputTokens);
        setModelKeys([...retained.map(item => item.key), ...models.map(() => nextKey.current++)]);
        setConfig(c => ({ ...c, models: [...retained.map(item => item.model), ...models] }));
        setExpandedModel(null); toast.success(`${models.length} modelos importados. Salve o provedor para aplicar.`);
      }} />}
      <div className="flex min-w-0 flex-col gap-3">{config.models.map((model, index) => <CustomModelEditor key={modelKeys[index]} model={model} index={index} baseUrl={config.baseUrl} protocol={config.protocol} saving={saving} canRemove={config.models.length > 1}
        open={expandedModel === modelKeys[index]} onOpenChange={open => setExpandedModel(open ? modelKeys[index] : null)}
        onRemove={() => { markPending(modelKeys[index], false); if (expandedModel === modelKeys[index]) setExpandedModel(null); setModelKeys(keys => keys.filter((_, i) => i !== index)); setConfig(c => ({ ...c, models: c.models.filter((_, i) => i !== index) })); }}
        onChange={model => setConfig(c => ({ ...c, models: c.models.map((old, i) => i === index ? model : old) }))}
        onPendingChange={busy => markPending(modelKeys[index], busy)}
        onTokenField={tokenField => setConfig(c => ({ ...c, tokenField }))}
      />)}</div>
    </fieldset>
    {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
    <div className="flex justify-end gap-2 border-t pt-3"><Button type="button" variant="ghost" disabled={saving} onClick={onCancel}>Cancelar</Button><Button type="submit" disabled={saving || pending.size > 0 || discovering}>{saving ? "Salvando…" : "Salvar provedor"}</Button></div>
  </form></TooltipProvider>;
}
