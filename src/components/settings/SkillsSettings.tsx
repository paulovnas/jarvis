import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowUpCircle, BookOpen, RefreshCw, Search, Store, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { AlertDialog, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Spinner } from "@/components/ui/spinner";
import { skillsSnapshotSchema, skillUpdateSchema, skillError, type Skill, type SkillSnapshot } from "@/core/skills";
import { SkillDetailsDialog, type SkillSelection } from "./SkillDetailsDialog";
import { SkillsMarketplace } from "./SkillsMarketplace";

const ORIGINS = { jarvis: "Jarvis", agents: ".agents · Global", project: ".agents · Projeto" };
export function SkillsSettings({ onCountChange }: { onCountChange?: (count: number) => void }) {
  const [snapshot, setSnapshot] = useState<SkillSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [marketplace, setMarketplace] = useState(false);
  const [selection, setSelection] = useState<SkillSelection | null>(null);
  const [deleting, setDeleting] = useState<Skill | null>(null);
  const mounted = useRef(false);
  const pending = useRef(false);
  const update = useCallback((value: unknown) => {
    const parsed = skillsSnapshotSchema.parse(value);
    if (mounted.current) { setSnapshot(parsed); onCountChange?.(parsed.skills.length); setError(null); }
    return parsed;
  }, [onCountChange]);
  useEffect(() => {
    mounted.current = true;
    let active = true;
    void invoke("list_skills").then(async value => {
      if (!active) return;
      const parsed = update(value);
      if (parsed.skills.some(skill => skill.marketplaceId)) {
        pending.current = true; setBusy("check");
        try { const checked = await invoke("check_skill_updates"); if (active) update(checked); }
        catch (cause) { if (active) toast.error(skillError(cause)); }
        finally { pending.current = false; if (active) setBusy(null); }
      }
    }).catch(cause => { if (active) setError(skillError(cause)); });
    return () => { active = false; mounted.current = false; };
  }, [update]);
  async function perform(key: string, action: () => Promise<void>) {
    if (pending.current) return;
    pending.current = true; setBusy(key);
    try { await action(); } catch (cause) { if (mounted.current) toast.error(skillError(cause)); }
    finally { pending.current = false; if (mounted.current) setBusy(null); }
  }
  async function upgrade(ids: string[]) {
    await perform("update", async () => {
      const result = skillUpdateSchema.parse(await invoke("update_skills", { ids })); update(result.snapshot);
      if (result.updated) toast.success(result.updated === 1 ? "Skill atualizada" : `${result.updated} skills atualizadas`);
      for (const error of result.errors) toast.error(error);
    });
  }
  const upgrades = snapshot?.skills.filter(skill => skill.updateAvailable) ?? [];
  const visible = snapshot?.skills.filter(skill => `${skill.name} ${skill.description} ${skill.source ?? ""}`.toLowerCase().includes(query.toLowerCase().trim())) ?? [];
  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <h2 className="text-sm font-medium">Skills</h2>
      <div className="flex items-center gap-1">
        {upgrades.length > 1 && <Button variant="outline" size="sm" className="cursor-pointer" disabled={!!busy} onClick={() => { void upgrade(upgrades.map(skill => skill.id)); }}><ArrowUpCircle aria-hidden="true" />Atualizar todas<Badge variant="secondary">{upgrades.length}</Badge></Button>}
        <Button variant="ghost" size="icon-sm" className="cursor-pointer" title="Verificar atualizações" aria-label="Verificar atualizações de skills" disabled={!!busy} onClick={() => { void perform("check", async () => { update(await invoke("check_skill_updates")); }); }}>{busy === "check" ? <Spinner /> : <RefreshCw />}</Button>
        <Button size="sm" className="cursor-pointer" onClick={() => setMarketplace(true)}><Store aria-hidden="true" />Marketplace</Button>
      </div>
    </div>
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border bg-card px-3 py-2.5">
      <Label htmlFor="skills-agents" className="cursor-pointer text-xs">Incluir .agents/skills</Label>
      <Switch id="skills-agents" className="cursor-pointer" checked={snapshot?.includeAgents ?? false} disabled={!snapshot || !!busy} onCheckedChange={enabled => { void perform("agents", async () => { update(await invoke("set_skills_agents", { enabled })); }); }} />
    </div>
    {snapshot && <span className="truncate font-mono text-[11px] text-muted-foreground" title={snapshot.directory}>{snapshot.directory}</span>}
    <div className="relative"><Search aria-hidden="true" className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground" /><Input aria-label="Buscar skills instaladas" value={query} onChange={event => setQuery(event.target.value)} placeholder="Buscar skills" className="pl-9" /></div>
    {busy === "update" && <span role="status" className="flex items-center gap-2 text-xs text-muted-foreground"><Spinner />Atualizando skills…</span>}
    {error ? <p role="alert" className="text-xs text-destructive">{error}</p> : !snapshot || busy === "agents" ? <CardsSkeleton label="Carregando skills" columns /> : visible.length === 0 ? <p className="py-8 text-center text-sm text-muted-foreground">{query ? "Nenhuma skill encontrada." : "Nenhuma skill instalada."}</p> : <div className="grid grid-cols-1 items-start gap-3 sm:grid-cols-2">{visible.map(skill => <Card key={skill.id} className="min-w-0 gap-0 overflow-hidden p-0">
      <div className="flex items-center gap-2 p-3">
        <Button variant="ghost" className="h-auto min-w-0 flex-1 cursor-pointer justify-start gap-3 p-0 text-left hover:bg-transparent" aria-label={`Detalhes de ${skill.name}`} onClick={() => setSelection({ id: skill.id, name: skill.name })}>
          <BookOpen aria-hidden="true" className="size-4 shrink-0 text-primary" />
          <span className="flex min-w-0 flex-1 flex-col gap-1"><span className="truncate text-sm font-medium">{skill.name}</span><span className="line-clamp-2 whitespace-normal text-xs font-normal text-muted-foreground">{skill.description}</span><span className="mt-1 flex flex-wrap gap-1"><Badge variant="outline" className="text-[10px]">{ORIGINS[skill.origin]}</Badge><Badge variant="outline" className={skill.enabled ? "border-[#98c379]/30 text-[#98c379]" : "text-muted-foreground"}>{skill.enabled ? "Ativa" : "Inativa"}</Badge>{skill.updateError && <Badge variant="outline" className="border-[#e5c07b]/30 text-[#e5c07b]" title={skill.updateError}>Verificação pendente</Badge>}</span></span>
        </Button>
        {skill.updateAvailable && <Button variant="ghost" size="icon-sm" title={`Atualizar ${skill.name}`} aria-label={`Atualizar ${skill.name}`} className="cursor-pointer text-primary" disabled={!!busy} onClick={() => { void upgrade([skill.id]); }}><ArrowUpCircle /></Button>}
        <Switch aria-label={`Ativar ${skill.name}`} className="cursor-pointer" checked={skill.enabled} disabled={!!busy} onCheckedChange={enabled => { void perform(skill.id, async () => { update(await invoke("set_skill_enabled", { id: skill.id, enabled })); toast.success(enabled ? "Skill ativada" : "Skill desativada"); }); }} />
        <Button variant="ghost" size="icon-sm" className="shrink-0 cursor-pointer text-muted-foreground hover:text-destructive" aria-label={`Excluir ${skill.name}`} title={`Excluir ${skill.name}`} disabled={!!busy} onClick={() => setDeleting(skill)}><Trash2 /></Button>
      </div>
    </Card>)}</div>}
    {snapshot?.warnings.map(warning => <p key={warning} role="alert" className="text-xs text-[#e5c07b]">{warning}</p>)}
    <SkillsMarketplace open={marketplace} onOpenChange={setMarketplace} installed={snapshot?.skills ?? []} onInstalled={update} />
    <SkillDetailsDialog selection={selection} onClose={() => setSelection(null)} />
    <AlertDialog open={deleting !== null} onOpenChange={open => { if (!open && !pending.current) setDeleting(null); }}>
      <AlertDialogContent><AlertDialogHeader><AlertDialogTitle>Excluir {deleting?.name}?</AlertDialogTitle><AlertDialogDescription>{deleting?.linked ? "O vínculo será removido. A pasta de destino será preservada." : deleting?.origin === "jarvis" ? "A pasta da skill e seus arquivos serão excluídos definitivamente." : "A pasta compartilhada será excluída definitivamente, afetando também outros agentes que a utilizam."}</AlertDialogDescription></AlertDialogHeader>
        <p className="break-all font-mono text-xs text-muted-foreground">{deleting?.removalPath ?? deleting?.path}</p>
        <AlertDialogFooter><Button variant="ghost" className="cursor-pointer" disabled={!!busy} onClick={() => setDeleting(null)}>Cancelar</Button><Button variant="destructive" className="cursor-pointer" disabled={!!busy} onClick={() => { if (deleting) void perform("delete", async () => { update(await invoke("delete_skill", { id: deleting.id })); setDeleting(null); toast.success("Skill excluída"); }); }}>{busy === "delete" && <Spinner />}Excluir skill</Button></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>;
}
