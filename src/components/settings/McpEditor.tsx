import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ChevronDown, Code, ExternalLink, Plus, Settings2, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Hint } from "@/components/ui/hint";
import { Input, Textarea } from "@/components/TextInput";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { MCP_TEMPLATE } from "@/core/mcp";
import { emptyMcpDraft, readMcpDraft, writeMcpDraft, type McpDraft, type McpPair } from "@/core/mcp-form";

export function McpEditor({ initialValue, busy, error, onSave, onCancel }: {
  initialValue: string; busy: boolean; error: string | null; onSave: (raw: string) => void; onCancel: () => void;
}) {
  const [initial] = useState(() => {
    try { return { draft: readMcpDraft(initialValue), error: null }; }
    catch (cause) { return { draft: emptyMcpDraft(), error: (cause as Error).message }; }
  });
  const [draft, setDraft] = useState(initial.draft);
  const [raw, setRaw] = useState(initialValue);
  const [mode, setMode] = useState(initial.error ? "json" : "visual");
  const [validation, setValidation] = useState<string | null>(initial.error);
  function update(patch: Partial<McpDraft>) { setDraft(value => ({ ...value, ...patch })); setValidation(null); }
  function switchMode(next: unknown) {
    if (busy || next === mode) return;
    try {
      if (next === "json") { setRaw(writeMcpDraft(draft)); setMode("json"); }
      else if (next === "visual") { setDraft(readMcpDraft(raw)); setMode("visual"); }
      setValidation(null);
    } catch (cause) { setValidation((cause as Error).message); }
  }
  function submit() {
    try { const value = mode === "visual" ? writeMcpDraft(draft, true) : raw; setValidation(null); onSave(value); }
    catch (cause) { setValidation((cause as Error).message); }
  }
  const issue = validation ?? error;
  return <form className="flex min-h-0 min-w-0 flex-col gap-5" noValidate onSubmit={event => { event.preventDefault(); submit(); }}>
    <Tabs value={mode} onValueChange={switchMode} className="min-h-0 overflow-y-auto">
      <TabsList className="w-full" aria-label="Forma de configurar MCP">
        <TabsTrigger value="visual" disabled={busy}><Settings2 aria-hidden="true" />Visual</TabsTrigger>
        <TabsTrigger value="json" disabled={busy}><Code aria-hidden="true" />JSON</TabsTrigger>
      </TabsList>
      <TabsContent value="visual" className="pt-3">
        <FieldGroup className="gap-5">
          <FieldGroup className="grid gap-4 sm:grid-cols-2">
            <Field><FieldLabel htmlFor="mcp-name">Nome do MCP</FieldLabel><Input id="mcp-name" value={draft.name} onChange={e => update({ name: e.target.value })} placeholder="ex.: minha-documentacao" maxLength={48} disabled={busy} autoComplete="off" /><FieldDescription>Letras, números, hífens ou sublinhados.</FieldDescription></Field>
            <Field><FieldLabel htmlFor="mcp-kind">Conexão</FieldLabel><Select value={draft.type} disabled={busy} onValueChange={type => { if (type === "local" || type === "remote") update({ type }); }}><SelectTrigger id="mcp-kind" className="w-full"><SelectValue>{draft.type === "local" ? "Programa local (stdio)" : "Servidor remoto (HTTP)"}</SelectValue></SelectTrigger><SelectContent><SelectGroup><SelectItem value="local">Programa local (stdio)</SelectItem><SelectItem value="remote">Servidor remoto (HTTP)</SelectItem></SelectGroup></SelectContent></Select><FieldDescription>{draft.type === "local" ? "O Jarvis inicia o programa no seu computador." : "O Jarvis se conecta ao endereço informado."}</FieldDescription></Field>
          </FieldGroup>
          {draft.type === "local" ? <>
            <Field><FieldLabel htmlFor="mcp-command">Programa</FieldLabel><Input id="mcp-command" value={draft.command} onChange={e => update({ command: e.target.value })} placeholder="ex.: npx, uvx ou caminho do executável" disabled={busy} autoComplete="off" spellCheck={false} /><FieldDescription>Informe apenas o executável. Adicione as opções e o pacote nos argumentos abaixo.</FieldDescription></Field>
            <FieldSet className="gap-2"><FieldLegend variant="label">Argumentos</FieldLegend><FieldDescription>Uma opção ou valor por linha, na ordem do comando. Não adicione aspas ao redor.</FieldDescription>
              {draft.args.map((arg, index) => <div key={index} className="flex items-center gap-2"><Input aria-label={`Argumento ${index + 1}`} value={arg} onChange={e => update({ args: draft.args.map((value, i) => i === index ? e.target.value : value) })} disabled={busy} spellCheck={false} autoComplete="off" /><Hint content={`Remover argumento ${index + 1}`}><Button type="button" variant="ghost" size="icon-sm" aria-label={`Remover argumento ${index + 1}`} disabled={busy} onClick={() => update({ args: draft.args.filter((_, i) => i !== index) })}><Trash2 /></Button></Hint></div>)}
              <Button type="button" variant="outline" size="sm" className="self-start" disabled={busy || draft.args.length >= 127} onClick={() => update({ args: [...draft.args, ""] })}><Plus data-icon="inline-start" />Adicionar argumento</Button>
            </FieldSet>
            <PairFields title="Variáveis de ambiente" singular="variável" description="Chaves e configurações que o programa precisa, como API_KEY." values={draft.environment} onChange={environment => update({ environment })} disabled={busy} />
          </> : <>
            <Field><FieldLabel htmlFor="mcp-url">URL do servidor</FieldLabel><Input id="mcp-url" value={draft.url} onChange={e => update({ url: e.target.value })} placeholder="https://exemplo.com/mcp" disabled={busy} autoComplete="off" spellCheck={false} /><FieldDescription>Use o endereço MCP fornecido pelo serviço.</FieldDescription></Field>
            <PairFields title="Cabeçalhos de autenticação" singular="cabeçalho" description="Se o serviço exigir uma chave, use por exemplo Authorization e o valor Bearer seguido da chave." values={draft.headers} onChange={headers => update({ headers })} disabled={busy} />
          </>}
          <Collapsible><CollapsibleTrigger render={<Button type="button" variant="ghost" size="sm" />} className="group w-full justify-between">Opções avançadas<ChevronDown data-icon="inline-end" className="group-aria-expanded:rotate-180" /></CollapsibleTrigger><CollapsibleContent>
            <FieldGroup className="gap-4 pt-4">
              {draft.type === "local" && <Field><FieldLabel htmlFor="mcp-cwd">Pasta de trabalho (opcional)</FieldLabel><Input id="mcp-cwd" value={draft.cwd} onChange={e => update({ cwd: e.target.value })} disabled={busy} placeholder="Pasta em que o programa será iniciado" /><FieldDescription>Deixe em branco para usar a pasta padrão do Jarvis.</FieldDescription></Field>}
              <FieldGroup className="grid gap-4 sm:grid-cols-2"><Field><FieldLabel htmlFor="mcp-timeout">Tempo para conectar (ms)</FieldLabel><Input id="mcp-timeout" type="number" min={1000} max={120000} value={draft.timeout} onChange={e => update({ timeout: e.target.value })} disabled={busy} /></Field><Field><FieldLabel htmlFor="mcp-request-timeout">Tempo por chamada (ms)</FieldLabel><Input id="mcp-request-timeout" type="number" min={1000} max={900000} value={draft.requestTimeout} onChange={e => update({ requestTimeout: e.target.value })} disabled={busy} /></Field></FieldGroup>
            </FieldGroup>
          </CollapsibleContent></Collapsible>
          <Field orientation="horizontal"><Switch id="mcp-editor-enabled" checked={draft.enabled} onCheckedChange={enabled => update({ enabled })} disabled={busy} /><FieldLabel htmlFor="mcp-editor-enabled">MCP ativado</FieldLabel></Field>
        </FieldGroup>
      </TabsContent>
      <TabsContent value="json" className="pt-3">
        <FieldGroup className="gap-3"><Field data-invalid={!!issue}><FieldLabel htmlFor="mcp-json">Configuração JSON</FieldLabel><Textarea id="mcp-json" value={raw} onChange={e => { setRaw(e.target.value); setValidation(null); }} placeholder={MCP_TEMPLATE} disabled={busy} autoComplete="off" spellCheck={false} maxLength={65536} aria-invalid={!!issue} className="min-h-64 resize-y font-mono text-xs" /><FieldDescription>Um MCP nomeado no formato OpenCode, sem a chave externa mcp.</FieldDescription></Field>
          <Button type="button" variant="link" className="h-auto justify-start p-0" onClick={() => { void openUrl("https://opencode.ai/docs/mcp-servers/").catch(() => toast.error("Não foi possível abrir a documentação")); }}><ExternalLink data-icon="inline-start" />Como configurar MCPs no OpenCode</Button>
        </FieldGroup>
      </TabsContent>
    </Tabs>
    {issue && <p role="alert" className="text-sm text-destructive">{issue}</p>}
    <DialogFooter><Button type="button" variant="ghost" disabled={busy} onClick={onCancel}>Cancelar</Button><Button type="submit" disabled={busy}>{busy && <Spinner data-icon="inline-start" />}Salvar MCP</Button></DialogFooter>
  </form>;
}

function PairFields({ title, singular, description, values, onChange, disabled }: {
  title: string; singular: string; description: string; values: McpPair[]; onChange: (values: McpPair[]) => void; disabled: boolean;
}) {
  return <FieldSet className="gap-2"><FieldLegend variant="label">{title}</FieldLegend><FieldDescription>{description}</FieldDescription>
    {values.map((pair, index) => <div key={index} className="flex items-start gap-2"><FieldGroup className="grid flex-1 gap-2 sm:grid-cols-2">
      <Field><FieldLabel className="sr-only" htmlFor={`mcp-${singular}-key-${index}`}>Nome {singular === "variável" ? "da" : "do"} {singular} {index + 1}</FieldLabel><Input id={`mcp-${singular}-key-${index}`} placeholder="Nome" value={pair.key} disabled={disabled} autoComplete="off" spellCheck={false} onChange={e => onChange(values.map((item, i) => i === index ? { ...item, key: e.target.value } : item))} /></Field>
      <Field><FieldLabel className="sr-only" htmlFor={`mcp-${singular}-value-${index}`}>Valor {singular === "variável" ? "da" : "do"} {singular} {index + 1}</FieldLabel><Input id={`mcp-${singular}-value-${index}`} type="password" placeholder="Valor" value={pair.value} disabled={disabled} autoComplete="off" spellCheck={false} onChange={e => onChange(values.map((item, i) => i === index ? { ...item, value: e.target.value } : item))} /></Field>
    </FieldGroup><Hint content={`Remover ${singular} ${index + 1}`}><Button type="button" variant="ghost" size="icon-sm" aria-label={`Remover ${singular} ${index + 1}`} disabled={disabled} onClick={() => onChange(values.filter((_, i) => i !== index))}><Trash2 /></Button></Hint></div>)}
    <Button type="button" variant="outline" size="sm" className="self-start" disabled={disabled} onClick={() => onChange([...values, { key: "", value: "" }])}><Plus data-icon="inline-start" />Adicionar {singular}</Button>
  </FieldSet>;
}
