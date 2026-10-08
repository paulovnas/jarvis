import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronDown, FolderOpen } from "lucide-react";
import { pluginDraftSchema, type PluginDraft, type PluginOperation } from "@/core/plugins";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input, Textarea } from "@/components/TextInput";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { DialogFooter } from "@/components/ui/dialog";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";

type Props = { busy: boolean; error: string | null; onSubmit: (operation: PluginOperation) => void; onCancel: () => void };

export function PluginSourceForm({ busy, error, onSubmit, onCancel }: Props) {
  const [kind, setKind] = useState("github");
  const [source, setSource] = useState("");
  const [ref, setRef] = useState("");
  const [sparse, setSparse] = useState("");
  const [invalid, setInvalid] = useState<string | null>(null);
  const [choosing, setChoosing] = useState(false);
  async function choose() {
    setChoosing(true);
    try { const path = await open({ title: "Selecionar marketplace local", directory: true, multiple: false }); if (typeof path === "string") setSource(path); }
    catch { setInvalid("Não foi possível selecionar a pasta."); }
    finally { setChoosing(false); }
  }
  return <form className="flex min-h-0 flex-col gap-4" onSubmit={event => { event.preventDefault(); if (!source.trim()) { setInvalid("Informe a origem do marketplace."); return; } const origin = kind === "github" && !source.includes("://") ? `https://github.com/${source.trim().replace(/\.git$/, "")}.git` : source.trim(); onSubmit({ action: "addMarketplace", source: origin, refName: ref.trim() || null, sparsePaths: sparse.split("\n").map(value => value.trim()).filter(Boolean) }); }}>
    <FieldGroup className="min-h-0 gap-4 overflow-y-auto px-1 pb-1">
      <Field><FieldLabel htmlFor="plugin-source-kind">Tipo de origem</FieldLabel><Select value={kind} onValueChange={value => { if (value) setKind(value); }} disabled={busy}><SelectTrigger id="plugin-source-kind" className="w-full cursor-pointer"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="github" className="cursor-pointer">GitHub</SelectItem><SelectItem value="git" className="cursor-pointer">Repositório Git</SelectItem><SelectItem value="local" className="cursor-pointer">Pasta local</SelectItem></SelectGroup></SelectContent></Select></Field>
      <Field><FieldLabel htmlFor="plugin-source">{kind === "local" ? "Pasta do marketplace" : "Origem do marketplace"}</FieldLabel><div className="flex gap-2"><Input id="plugin-source" className="min-w-0 flex-1 font-mono" value={source} onChange={event => { setSource(event.target.value); setInvalid(null); }} disabled={busy || choosing} placeholder={kind === "github" ? "organização/repositório" : kind === "git" ? "https://…/repositorio.git" : "/caminho/marketplace"} />{kind === "local" && <Button type="button" variant="outline" size="icon" aria-label="Selecionar pasta do marketplace" className="cursor-pointer" disabled={busy || choosing} onClick={() => { void choose(); }}><FolderOpen /></Button>}</div><FieldDescription>O Jarvis usa o manifesto do marketplace para listar os plugins. Você revisará o conteúdo antes de instalar.</FieldDescription></Field>
      {kind !== "local" && <><Field><FieldLabel htmlFor="plugin-ref">Branch, tag ou commit (opcional)</FieldLabel><Input id="plugin-ref" className="font-mono" value={ref} onChange={event => setRef(event.target.value)} disabled={busy} placeholder="main" /></Field><Field><FieldLabel htmlFor="plugin-sparse">Pastas para checkout parcial (opcional)</FieldLabel><Textarea id="plugin-sparse" className="font-mono" value={sparse} onChange={event => setSparse(event.target.value)} disabled={busy} placeholder="Uma pasta por linha" /><FieldDescription>Deixe vazio para buscar o repositório completo.</FieldDescription></Field></>}
      {(invalid ?? error) && <p role="alert" className="text-sm text-destructive">{invalid ?? error}</p>}
    </FieldGroup>
    <DialogFooter className="shrink-0"><Button type="button" variant="ghost" className="cursor-pointer" disabled={busy || choosing} onClick={onCancel}>Cancelar</Button><Button type="submit" className="cursor-pointer" disabled={busy || choosing}>{busy && <Spinner data-icon="inline-start" />}Revisar marketplace</Button></DialogFooter>
  </form>;
}

export function PluginCreateForm({ busy, error, onSubmit, onCancel }: Props) {
  const [name, setName] = useState(""); const [description, setDescription] = useState("");
  const [skillName, setSkillName] = useState(""); const [content, setContent] = useState("");
  const [advanced, setAdvanced] = useState(""); const [invalid, setInvalid] = useState<string | null>(null);
  function submit() {
    try {
      let extra: Partial<PluginDraft> = {};
      if (advanced.trim()) { const raw: unknown = JSON.parse(advanced); if (typeof raw !== "object" || raw === null || Array.isArray(raw)) throw new Error(); extra = pluginDraftSchema.partial().strict().parse(raw); }
      const draft = pluginDraftSchema.parse({ skills: skillName.trim() || content.trim() ? [{ name: skillName.trim(), content }] : [], mcpServers: {}, hooks: null, apps: {}, files: [], ...extra, name: name.trim(), description: description.trim() });
      if (!draft.skills.length && !Object.keys(draft.mcpServers).length && !draft.hooks && !Object.keys(draft.apps).length) { setInvalid("Adicione uma skill ou um componente na configuração avançada."); return; }
      setInvalid(null); onSubmit({ action: "create", draft });
    } catch { setInvalid("Confira o nome, o conteúdo da skill e o JSON da configuração avançada."); }
  }
  return <form className="flex min-h-0 flex-col gap-4" onSubmit={event => { event.preventDefault(); submit(); }}>
    <FieldGroup className="min-h-0 gap-4 overflow-y-auto px-1 pb-1">
      <Field><FieldLabel htmlFor="plugin-name">Nome do plugin</FieldLabel><Input id="plugin-name" className="font-mono" value={name} onChange={event => setName(event.target.value)} maxLength={64} disabled={busy} placeholder="meu-plugin" /><FieldDescription>Use letras, números, ponto, hífen ou sublinhado. O pacote será gerenciado pelo Jarvis. Você também pode pedir ao agente para preparar um plugin.</FieldDescription></Field>
      <Field><FieldLabel htmlFor="plugin-description">Descrição</FieldLabel><Input id="plugin-description" value={description} onChange={event => setDescription(event.target.value)} disabled={busy} /></Field>
      <Field><FieldLabel htmlFor="plugin-skill-name">Nome da skill (opcional)</FieldLabel><Input id="plugin-skill-name" className="font-mono" value={skillName} onChange={event => setSkillName(event.target.value)} disabled={busy} placeholder="minha-skill" /></Field>
      <Field><FieldLabel htmlFor="plugin-skill-content">Conteúdo de SKILL.md</FieldLabel><Textarea id="plugin-skill-content" className="min-h-40 font-mono" value={content} onChange={event => setContent(event.target.value)} disabled={busy} placeholder={"---\nname: minha-skill\ndescription: Quando usar esta skill.\n---\nInstruções…"} /><FieldDescription>Inclua o frontmatter com name e description, seguido das instruções.</FieldDescription></Field>
      <Collapsible className="flex flex-col gap-3"><CollapsibleTrigger render={<Button variant="outline" type="button" />} className="cursor-pointer justify-between" disabled={busy}>Configuração avançada<ChevronDown data-icon="inline-end" /></CollapsibleTrigger><CollapsibleContent><Field><FieldLabel htmlFor="plugin-advanced">Componentes em JSON</FieldLabel><Textarea id="plugin-advanced" className="min-h-36 font-mono" value={advanced} onChange={event => setAdvanced(event.target.value)} disabled={busy} placeholder={'{"mcpServers": {}, "hooks": null, "apps": {}, "files": []}'} /><FieldDescription>Campos opcionais: skills, mcpServers, hooks, apps e files (path/content). Os comandos de hooks exigem uma autorização separada.</FieldDescription></Field></CollapsibleContent></Collapsible>
      {(invalid ?? error) && <p role="alert" className="text-sm text-destructive">{invalid ?? error}</p>}
    </FieldGroup>
    <DialogFooter className="shrink-0"><Button type="button" variant="ghost" className="cursor-pointer" disabled={busy} onClick={onCancel}>Cancelar</Button><Button type="submit" className="cursor-pointer" disabled={busy}>{busy && <Spinner data-icon="inline-start" />}Revisar plugin</Button></DialogFooter>
  </form>;
}
