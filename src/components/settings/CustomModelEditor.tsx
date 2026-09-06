import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
import { discoveredModelSchema, reasoningFormats, reasoningLabels, supportsModelLookup, type CustomConfig, type CustomModel } from "@/core/custom-provider";

type Lookup = { key: string; pending: boolean; source?: string; error?: string };
export function CustomModelEditor({ model, index, baseUrl, protocol, saving, canRemove, open, onOpenChange, onRemove, onChange, onPendingChange, onTokenField }: {
  model: CustomModel; index: number; baseUrl: string; protocol: CustomConfig["protocol"]; saving: boolean; canRemove: boolean;
  open: boolean; onOpenChange: (open: boolean) => void;
  onRemove: () => void; onChange: (model: CustomModel) => void; onPendingChange: (pending: boolean) => void; onTokenField: (field: CustomConfig["tokenField"]) => void;
}) {
  const number = index + 1;
  const key = `${baseUrl}\n${protocol}\n${model.id}\n${index}`;
  const canLookup = supportsModelLookup(baseUrl);
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
    if (!canLookup || !model.id.trim() || saving || (!force && (activeLookup || (initialId.current === model.id && model.contextWindow > 0)))) return;
    const version = ++request.current;
    setLookup({ key, pending: true }); onPendingChange(true);
    try {
      const result = discoveredModelSchema.parse(await invoke("lookup_custom_model", { baseUrl, protocol, modelId: model.id }));
      if (version !== request.current) return;
      if (result.model.id !== model.id || new URL(result.sourceUrl).origin !== "https://openrouter.ai") throw new Error("Unexpected catalog result");
      const previous = model.defaultReasoningLevel;
      onChange({ ...result.model, defaultReasoningLevel: previous && result.model.reasoningLevels.includes(previous) ? previous : result.model.defaultReasoningLevel });
      onTokenField(result.tokenField);
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
        <Button type="button" variant="ghost" size="icon-sm" aria-label={`Remover modelo ${number}`} disabled={!canRemove || saving} onClick={onRemove}><Trash2 data-icon="inline-start" className="text-destructive" /></Button>
      </CardAction>
    </CardHeader>
    {!open && loading && <div role="status" aria-label={`Buscando dados do modelo ${number}`} className="px-3 pt-2"><Skeleton className="h-3 w-32" /></div>}
    {!open && activeLookup?.error && <p role="alert" className="px-3 pt-2 text-xs text-destructive">{activeLookup.error}</p>}
    <CollapsibleContent>
    <CardContent className="flex flex-col gap-3 pt-3">
      <div className="flex items-end gap-2"><div className="min-w-0 flex-1"><Field label={`ID do modelo ${number}`} help="Copie o ID da página do modelo no OpenRouter (por exemplo, fabricante/modelo). Ao sair deste campo, o Jarvis consulta o catálogo público; não envia mensagens nem usa sua chave." value={model.id} disabled={saving} placeholder="fabricante/modelo" onChange={e => { request.current += 1; onPendingChange(false); setLookup(null); edit({ id: e.target.value }); }} onBlur={() => void lookupModel()} /></div>{canLookup && <Button type="button" variant="outline" size="icon" aria-label={`Preencher modelo ${number} pelo ID`} title="Preencher pelo ID" disabled={saving || loading || !model.id.trim()} onClick={() => void lookupModel(true)}><Sparkles className="size-3.5 text-primary" /></Button>}</div>
      {loading ? <div role="status" aria-label={`Buscando dados do modelo ${number}`} className="flex gap-2"><Skeleton className="h-5 w-28" /><Skeleton className="h-5 w-24" /><Skeleton className="h-5 w-20" /></div> : model.contextWindow > 0 && <div className="flex flex-wrap items-center gap-1.5 text-[10px]">
        <Badge variant="outline" className="font-mono text-[10px]">{model.contextWindow.toLocaleString("pt-BR")} contexto</Badge><Badge variant="outline" className="font-mono text-[10px]">{model.maxOutputTokens.toLocaleString("pt-BR")} saída</Badge>{model.supportsTools && <Badge variant="outline" className="text-[10px]">Ferramentas</Badge>}{model.supportsImages && <Badge variant="outline" className="text-[10px]">Imagens</Badge>}{model.reasoning !== "none" && model.reasoningLevels.length > 0 && <Badge variant="outline" title={model.reasoningLevels.join(", ")}>Raciocínio · {model.reasoningLevels.join(" / ")}</Badge>}
        {activeLookup?.source && <Button type="button" variant="link" size="sm" className="h-5 px-1 text-[10px]" onClick={() => { void openUrl(activeLookup.source!).catch(() => toast.error("Não foi possível abrir o catálogo.")); }}>OpenRouter<ExternalLink className="size-3" /></Button>}
      </div>}
      {activeLookup?.error && <p role="alert" className="text-xs text-destructive">{activeLookup.error}</p>}
      <Collapsible open={detailsOpen} onOpenChange={setManual}>
        <CollapsibleTrigger render={<Button type="button" variant="ghost" size="sm" />} className="group h-7 w-full cursor-pointer justify-start gap-2 px-0 text-xs text-muted-foreground" disabled={loading || saving}><SlidersHorizontal className="size-3.5" />Configurar manualmente<ChevronRight className={`ml-auto size-3.5 transition-transform ${detailsOpen ? "rotate-90" : ""}`} /></CollapsibleTrigger>
        <CollapsibleContent className="space-y-3 pt-3">
          <Field label={`Nome do modelo ${number}`} help="Nome que aparece no seletor do chat. Pode ser abreviado; o ID enviado à API não muda." value={model.name} disabled={saving || loading} placeholder="Nome no seletor" onChange={e => edit({ name: e.target.value })} />
          <div className="grid grid-cols-2 gap-3"><Field label={`Contexto (tokens) · modelo ${number}`} help="Total que cabe na conversa, incluindo entrada e saída. No OpenRouter, consulte Context ou context_length na página/API do modelo; usamos o menor limite informado pelo catálogo e pelo provedor principal." type="number" min={4096} max={100_000_000} value={model.contextWindow || ""} disabled={saving || loading} placeholder="Obrigatório" onChange={e => edit({ contextWindow: Number(e.target.value) })} /><Field label={`Saída máxima (tokens) · modelo ${number}`} help="Limite de tokens de uma resposta, incluindo raciocínio. Use Max output ou top_provider.max_completion_tokens na página/API do modelo. É diferente do contexto e precisa ser menor que ele." type="number" min={1} max={10_000_000} value={model.maxOutputTokens || ""} disabled={saving || loading} placeholder="Obrigatório" onChange={e => edit({ maxOutputTokens: Number(e.target.value) })} /></div>
          <div className="flex flex-wrap gap-x-4 gap-y-2">{([{ key: "supportsTools", label: "Ferramentas", help: "Ative se o modelo aceitar tool calling. No OpenRouter, veja se supported_parameters inclui tools. Necessário para ler/editar arquivos e executar os recursos do Jarvis." }, { key: "supportsImages", label: "Imagens", help: "Ative apenas para modelos que aceitem imagens de entrada. No OpenRouter, veja Input modalities: image. Habilita o uso deste modelo pelo Vision." }] as const).map(({ key, label, help }) => <div key={key} className="flex items-center gap-1"><label className="flex cursor-pointer items-center gap-2 text-xs"><Switch aria-label={`${label} · modelo ${number}`} checked={model[key]} disabled={saving || loading} onCheckedChange={checked => edit({ [key]: checked })} className="cursor-pointer" />{label}</label><FieldHelp label={`${label} · modelo ${number}`}>{help}</FieldHelp></div>)}</div>
          <Choice label={`Raciocínio · modelo ${number}`} help="Padrão do provedor não envia parâmetros extras. OpenRouter usa reasoning; Effort usa reasoning_effort (ou reasoning.effort em Responses). Os outros formatos são específicos da API. Consulte a documentação de Reasoning/Thinking do provedor." value={model.reasoning} items={reasoningFormats(protocol).map(value => ({ value, label: reasoningLabels[value] }))} onChange={reasoning => edit({ reasoning, reasoningLevels: reasoning === "budget" ? ["off", "on"] : [], defaultReasoningLevel: reasoning === "budget" ? "on" : null, thinkingBudget: null })} disabled={saving || loading} />
          {model.reasoning !== "none" && <div className="grid gap-3 sm:grid-cols-2">{model.reasoning === "budget" ? <Field label={`Orçamento de thinking · modelo ${number}`} help="Tokens reservados ao raciocínio na API Messages. Consulte Extended thinking na documentação Anthropic: mínimo de 1.024 e sempre abaixo da saída máxima." type="number" min={1024} value={model.thinkingBudget ?? ""} disabled={saving || loading} onChange={e => edit({ thinkingBudget: Number(e.target.value) })} /> : <Field label={`Níveis aceitos · modelo ${number}`} help="Valores exatos aceitos pelo modelo, separados por vírgula. No OpenRouter, veja reasoning.supported_efforts na API de modelos. Não copie níveis de outro modelo; na dúvida, use Padrão do provedor." value={model.reasoningLevels.join(",")} disabled={saving || loading} placeholder="low,medium,high" onChange={e => { const levels = e.target.value.replace(/ /g, "").split(","); edit({ reasoningLevels: levels, defaultReasoningLevel: model.defaultReasoningLevel && levels.includes(model.defaultReasoningLevel) ? model.defaultReasoningLevel : null }); }} />}<Choice label={`Raciocínio padrão · modelo ${number}`} help="Nível selecionado inicialmente no chat. Deve pertencer à lista de níveis aceitos. O catálogo informa default_effort quando disponível." value={model.defaultReasoningLevel} items={[...new Set(model.reasoningLevels)].filter(Boolean).map(value => ({ value, label: value }))} onChange={defaultReasoningLevel => edit({ defaultReasoningLevel })} disabled={saving || loading} /></div>}
        </CollapsibleContent>
      </Collapsible>
    </CardContent>
    </CollapsibleContent>
  </Card></Collapsible>;
}
