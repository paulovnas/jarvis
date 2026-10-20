import { useEffect, useRef, useState } from "react";
import { readResource } from "@/core/resource-request";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ChevronRight, ExternalLink, SlidersHorizontal, Sparkles, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Choice, Field, FieldHelp } from "./CustomProviderFields";
import { discoveredModelSchema, isOllamaBaseUrl, normalizeOllamaBaseUrl, reasoningFormats, reasoningLabels, supportsModelLookup, type CustomConfig, type CustomModel } from "@/core/custom-provider";
import { Hint } from "@/components/ui/hint";
import { selectableReasoningLevels } from "@/core/reasoning";

type Lookup = { key: string; pending: boolean; source?: string; error?: string };
export function CustomModelEditor({ model, index, baseUrl, protocol, saving, canRemove, open, onOpenChange, onRemove, onChange, onPendingChange, onTokenField }: {
  model: CustomModel; index: number; baseUrl: string; protocol: CustomConfig["protocol"]; saving: boolean; canRemove: boolean;
  open: boolean; onOpenChange: (open: boolean) => void;
  onRemove: () => void; onChange: (model: CustomModel) => void; onPendingChange: (pending: boolean) => void; onTokenField: (field: CustomConfig["tokenField"]) => void;
}) {
  const number = index + 1;
  const key = `${baseUrl}\n${protocol}\n${model.id}\n${index}`;
  const nativeOllama = protocol === "ollama";
  const canLookup = nativeOllama ? isOllamaBaseUrl(baseUrl) : supportsModelLookup(baseUrl);
  const initialId = useRef(model.id);
  const request = useRef(0);
  const [lookup, setLookup] = useState<Lookup | null>(null);
  const [manual, setManual] = useState(false);
  const activeLookup = lookup?.key === key ? lookup : null;
  const loading = activeLookup?.pending === true;
  const pendingCallback = useRef(onPendingChange);
  useEffect(() => { pendingCallback.current = onPendingChange; }, [onPendingChange]);
  useEffect(() => () => { request.current += 1; pendingCallback.current(false); }, [key]);
  const edit = (patch: Partial<CustomModel>) => onChange({ ...model, ...patch });
  async function lookupModel(force = false) {
    if (!canLookup || !model.id.trim() || saving || (!force && (activeLookup || initialId.current === model.id))) return;
    const version = ++request.current;
    setLookup({ key, pending: true }); onPendingChange(true);
    try {
      const result = discoveredModelSchema.parse(await readResource(nativeOllama ? "lookup_ollama_model" : "lookup_custom_model", nativeOllama ? { baseUrl: normalizeOllamaBaseUrl(baseUrl), modelId: model.id } : { baseUrl, protocol, modelId: model.id }));
      if (version !== request.current) return;
      if (result.model.id !== model.id || new URL(result.sourceUrl).origin !== (nativeOllama ? new URL(baseUrl).origin : "https://openrouter.ai")) throw new Error("O servidor retornou dados de outro modelo ou endereço. Configure manualmente ou tente novamente.");
      const previous = model.defaultReasoningLevel;
      onChange({ ...result.model, defaultReasoningLevel: previous && result.model.reasoningLevels.includes(previous) ? previous : result.model.defaultReasoningLevel });
      if (result.model.supportsChat) onTokenField(result.tokenField);
      setLookup({ key, pending: false, source: result.sourceUrl }); setManual(false);
    } catch (error) {
      if (version !== request.current) return;
      setLookup({ key, pending: false, error: typeof error === "object" && error !== null && "message" in error && typeof error.message === "string" ? error.message : "Não foi possível buscar o modelo. Configure manualmente ou tente novamente." });
      setManual(true);
    } finally { if (version === request.current) onPendingChange(false); }
  }
  const detailsOpen = manual;
  return <Collapsible open={open} onOpenChange={onOpenChange} className="min-w-0"><Card size="sm" className="gap-0">
    <CardHeader className="grid-cols-[minmax(0,1fr)_auto] items-center gap-2">
      <CardTitle className="min-w-0"><CollapsibleTrigger render={<Button type="button" variant="ghost" />} className="h-auto w-full min-w-0 cursor-pointer justify-start gap-2 px-0 py-1 text-left" disabled={saving} aria-label={`${open ? "Recolher" : "Expandir"} modelo ${number}`}>
        <ChevronRight data-icon="inline-start" className={open ? "rotate-90" : ""} />
        <span className="flex min-w-0 flex-col gap-1">
          <span className="truncate">{model.name || `Modelo ${number}`}</span>
          <span className="truncate font-mono text-[10px] font-normal text-muted-foreground">{model.id || "Novo modelo"}</span>
        </span>
      </CollapsibleTrigger></CardTitle>
      <CardAction className="row-span-1 self-center">
        <Hint content={`Remover modelo ${number}`}><Button type="button" variant="ghost" size="icon-sm" aria-label={`Remover modelo ${number}`} disabled={!canRemove || saving} onClick={onRemove}><Trash2 data-icon="inline-start" className="text-destructive" /></Button></Hint>
      </CardAction>
    </CardHeader>
    {!open && loading && <div role="status" aria-label={`Buscando dados do modelo ${number}`} className="px-3 pt-2"><Skeleton className="h-3 w-32" /></div>}
    {!open && activeLookup?.error && <p role="alert" className="px-3 pt-2 text-xs text-destructive">{activeLookup.error}</p>}
    <CollapsibleContent>
    <CardContent className="flex flex-col gap-3 pt-3">
      <div className="flex items-end gap-2"><div className="min-w-0 flex-1"><Field label={`ID do modelo ${number}`} help={nativeOllama ? "Informe o ID instalado no Ollama com a tag, como qwen3:14b. Ao sair deste campo, o Jarvis consulta apenas os metadados desse modelo no servidor configurado, sem gerar mensagens." : canLookup ? "Copie o ID exato do modelo. Ao sair deste campo, o Jarvis consulta o catálogo público do OpenRouter; não envia mensagens nem usa sua chave." : "Use o ID literal aceito pela API, incluindo espaços, maiúsculas e pontos (por exemplo, Opus 5.5). O Jarvis não transforma o ID em slug."} value={model.id} disabled={saving} placeholder={nativeOllama ? "qwen3:14b" : "ID exato da API"} onChange={e => { request.current += 1; onPendingChange(false); setLookup(null); edit({ id: e.target.value }); }} onBlur={() => void lookupModel()} /></div>{canLookup && <Hint content="Preencher pelo ID"><Button type="button" variant="outline" size="icon" aria-label={`Preencher modelo ${number} pelo ID`} disabled={saving || loading || !model.id.trim()} onClick={() => void lookupModel(true)}><Sparkles className="size-3.5 text-primary" /></Button></Hint>}</div>
      {loading ? <div role="status" aria-label={`Buscando dados do modelo ${number}`} className="flex gap-2"><Skeleton className="h-5 w-28" /><Skeleton className="h-5 w-24" /><Skeleton className="h-5 w-20" /></div> : (model.contextWindow > 0 || model.supportsImageGeneration) && <div className="flex flex-wrap items-center gap-1.5 text-[10px]">
        {model.supportsChat && <><Badge variant="outline" className="font-mono text-[10px]">{model.contextWindow.toLocaleString("pt-BR")} contexto</Badge><Badge variant="outline" className="font-mono text-[10px]">{model.maxOutputTokens.toLocaleString("pt-BR")} saída</Badge></>}{model.supportsTools && <Badge variant="outline" className="text-[10px]">Ferramentas</Badge>}{model.supportsImages && <Badge variant="outline" className="text-[10px]">Imagens de entrada</Badge>}{model.supportsImageGeneration && <Badge variant="outline" className="text-[10px]">Geração de imagens</Badge>}{model.reasoning !== "none" && model.reasoningLevels.length > 0 && <Badge variant="outline">Raciocínio · {model.reasoningLevels.join(" / ")}</Badge>}
        {activeLookup?.source && <Button type="button" variant="link" size="sm" className="h-5 px-1 text-[10px]" onClick={() => { void openUrl(activeLookup.source!).catch(() => toast.error("Não foi possível abrir o catálogo.")); }}>{nativeOllama ? "Ollama" : "OpenRouter"}<ExternalLink className="size-3" /></Button>}
      </div>}
      {activeLookup?.error && <p role="alert" className="text-xs text-destructive">{activeLookup.error}</p>}
      <Collapsible open={detailsOpen} onOpenChange={setManual}>
        <CollapsibleTrigger render={<Button type="button" variant="ghost" size="sm" />} className="group h-7 w-full cursor-pointer justify-start gap-2 px-0 text-xs text-muted-foreground" disabled={loading || saving}><SlidersHorizontal className="size-3.5" />Configurar manualmente<ChevronRight className={`ml-auto size-3.5 transition-transform ${detailsOpen ? "rotate-90" : ""}`} /></CollapsibleTrigger>
        <CollapsibleContent className="space-y-3 pt-3">
          <Field label={`Nome do modelo ${number}`} help="Nome que aparece nos seletores do Jarvis. Pode ser abreviado; o ID enviado à API não muda." value={model.name} disabled={saving || loading} placeholder="Nome no seletor" onChange={e => edit({ name: e.target.value })} />
          <div className="flex flex-wrap gap-x-4 gap-y-2">
            {!nativeOllama && <div className="flex items-center gap-1"><label className="flex cursor-pointer items-center gap-2 text-xs"><Switch aria-label={`Geração de imagens · modelo ${number}`} checked={model.supportsImageGeneration} disabled={saving || loading || !canLookup} onCheckedChange={checked => edit({ supportsImageGeneration: checked })} className="cursor-pointer" />Geração de imagens</label><FieldHelp label={`Geração de imagens · modelo ${number}`}>Disponível para modelos OpenRouter que geram imagens. Habilita este modelo em Gerar imagens; aceitar imagens de entrada não significa gerar imagens.</FieldHelp></div>}
            {(model.supportsImageGeneration || !model.supportsChat) && <label className="flex cursor-pointer items-center gap-2 text-xs"><Switch aria-label={`Usar no chat · modelo ${number}`} checked={model.supportsChat} disabled={saving || loading} onCheckedChange={checked => edit({ supportsChat: checked, ...(!checked ? { supportsTools: false, contextWindow: 0, maxOutputTokens: 0, reasoning: "none", reasoningLevels: [], defaultReasoningLevel: null, thinkingBudget: null } : {}) })} className="cursor-pointer" />Usar no chat</label>}
            {([{ key: "supportsTools", label: "Ferramentas", help: "Ative se o modelo aceitar tool calling. No OpenRouter, veja se supported_parameters inclui tools. Necessário para ler/editar arquivos e executar os recursos do Jarvis." }, { key: "supportsImages", label: "Imagens de entrada", help: "Ative se o modelo aceitar imagens como entrada ou referência. Modelos de chat com esta capacidade podem ser usados pelo Vision. Esta opção não habilita geração de imagens." }] as const).map(({ key, label, help }) => <div key={key} className="flex items-center gap-1"><label className="flex cursor-pointer items-center gap-2 text-xs"><Switch aria-label={`${label} · modelo ${number}`} checked={model[key]} disabled={saving || loading || (key === "supportsTools" && !model.supportsChat)} onCheckedChange={checked => edit({ [key]: checked })} className="cursor-pointer" />{label}</label><FieldHelp label={`${label} · modelo ${number}`}>{help}</FieldHelp></div>)}
          </div>
          {!model.supportsChat && <p className="text-xs text-muted-foreground">Este modelo não aparece no seletor do chat. Limites de tokens não se aplicam.</p>}
          {model.supportsChat && <>
            <div className="grid grid-cols-2 gap-3"><Field label={`Contexto (tokens) · modelo ${number}`} help={nativeOllama ? "Quantidade de tokens alocada pelo Ollama para a conversa (num_ctx), até o limite suportado pelo modelo. Aumente apenas se houver memória disponível." : "Limite de contexto informado pela API. No OpenRouter, consulte Context ou context_length; usamos o menor limite informado pelo catálogo e pelo provedor principal. Consulte a documentação para saber como entrada e saída compartilham esse limite."} type="number" min={4096} max={100_000_000} value={model.contextWindow || ""} disabled={saving || loading} placeholder="Obrigatório" onChange={e => edit({ contextWindow: Number(e.target.value) })} /><Field label={`Saída máxima (tokens) · modelo ${number}`} help={nativeOllama ? "Limite de tokens de uma resposta no Ollama (num_predict), incluindo raciocínio. A consulta usa até 4.096 tokens como padrão inicial; ajuste conforme a memória e o contexto alocado." : "Limite de tokens de uma resposta, incluindo raciocínio. Use Max output ou top_provider.max_completion_tokens na página/API do modelo. Algumas APIs publicam limites independentes de entrada e saída."} type="number" min={1} max={10_000_000} value={model.maxOutputTokens || ""} disabled={saving || loading} placeholder="Obrigatório" onChange={e => edit({ maxOutputTokens: Number(e.target.value) })} /></div>
            {nativeOllama && <p className="text-xs text-muted-foreground">Este contexto define a alocação num_ctx do Ollama. Valores maiores usam mais RAM/VRAM.</p>}
            <Choice label={`Raciocínio · modelo ${number}`} help={nativeOllama ? "Ativar/desativar envia think ao Ollama quando o modelo aceita raciocínio. Padrão do provedor usa a configuração do servidor." : "Padrão do provedor não envia parâmetros extras. OpenRouter usa reasoning; Effort usa reasoning_effort (ou reasoning.effort em Responses). Os outros formatos são específicos da API. Consulte a documentação de Reasoning/Thinking do provedor."} value={model.reasoning} items={reasoningFormats(protocol).map(value => ({ value, label: reasoningLabels[value] }))} onChange={reasoning => edit({ reasoning, reasoningLevels: reasoning === "budget" || reasoning === "toggle" ? ["off", "on"] : [], defaultReasoningLevel: reasoning === "budget" || reasoning === "toggle" ? "on" : null, thinkingBudget: null })} disabled={saving || loading} />
            {model.reasoning !== "none" && <div className="grid gap-3 sm:grid-cols-2">{model.reasoning === "budget" ? <Field label={`Orçamento de thinking · modelo ${number}`} help="Tokens reservados ao raciocínio na API Messages. Consulte Extended thinking na documentação Anthropic: mínimo de 1.024 e sempre abaixo da saída máxima." type="number" min={1024} value={model.thinkingBudget ?? ""} disabled={saving || loading} onChange={e => edit({ thinkingBudget: Number(e.target.value) })} /> : model.reasoning !== "toggle" && <Field label={`Níveis aceitos · modelo ${number}`} help="Valores exatos aceitos pelo modelo, separados por vírgula. No OpenRouter, veja reasoning.supported_efforts na API de modelos. Não copie níveis de outro modelo; na dúvida, use Padrão do provedor." value={model.reasoningLevels.join(",")} disabled={saving || loading} placeholder="low,medium,high" onChange={e => { const levels = e.target.value.replace(/ /g, "").split(","); edit({ reasoningLevels: levels, defaultReasoningLevel: model.defaultReasoningLevel && levels.includes(model.defaultReasoningLevel) ? model.defaultReasoningLevel : null }); }} />}<Choice label={`Raciocínio padrão · modelo ${number}`} help="Nível selecionado inicialmente no chat. Deve pertencer à lista de níveis aceitos. O catálogo informa default_effort quando disponível." value={model.defaultReasoningLevel} items={selectableReasoningLevels([...new Set(model.reasoningLevels)]).filter(Boolean).map(value => ({ value, label: value }))} onChange={defaultReasoningLevel => edit({ defaultReasoningLevel })} disabled={saving || loading} /></div>}
          </>}
        </CollapsibleContent>
      </Collapsible>
    </CardContent>
    </CollapsibleContent>
  </Card></Collapsible>;
}
