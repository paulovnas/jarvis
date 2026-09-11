import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, GitPullRequest, Save, Sparkles } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { libraryError } from "@/core/library";
import { PULL_REQUEST_MODE_LABELS, publicationSettingsSchema, type PublicationSettings, type PullRequestMode } from "@/core/publication";

type Draft = Pick<PublicationSettings, "publishPrompt" | "prMode" | "prPrompt">;

function draftOf(settings: PublicationSettings): Draft {
  return { publishPrompt: settings.publishPrompt, prMode: settings.prMode, prPrompt: settings.prPrompt };
}

function OptionsSkeleton() {
  return <div role="status" aria-label="Carregando opções do projeto" className="mx-auto w-full max-w-5xl space-y-4 p-6"><Skeleton className="h-24 w-full" /><Skeleton className="h-80 w-full" /></div>;
}

export function ProjectOptions({ projectId }: { projectId: string }) {
  const [settings, setSettings] = useState<PublicationSettings | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const generation = useRef(0);

  useEffect(() => {
    const request = ++generation.current;
    void invoke<unknown>("get_project_publication_settings", { projectId }).then(value => {
      if (generation.current !== request) return;
      const loaded = publicationSettingsSchema.parse(value);
      if (loaded.projectId !== projectId) throw new Error("Mismatched project settings");
      setSettings(loaded); setDraft(draftOf(loaded)); setError(null);
    }).catch(cause => {
      if (generation.current === request) setError(libraryError(cause, "Não foi possível carregar as opções de publicação."));
    });
    return () => { if (generation.current === request) generation.current += 1; };
  }, [projectId]);

  const changed = useMemo(() => settings && draft ? JSON.stringify(draft) !== JSON.stringify(draftOf(settings)) : false, [draft, settings]);
  if (error) return <div className="p-6"><Alert variant="destructive"><AlertTriangle /><AlertTitle>Opções indisponíveis</AlertTitle><AlertDescription>{error}</AlertDescription></Alert></div>;
  if (!settings || !draft) return <OptionsSkeleton />;

  const save = async () => {
    if (saving || !changed) return;
    setSaving(true);
    try {
      const saved = publicationSettingsSchema.parse(await invoke<unknown>("save_project_publication_settings", { projectId, settings: draft }));
      setSettings(saved); setDraft(draftOf(saved));
      toast.success("Opções de publicação salvas");
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível salvar as opções de publicação."));
    } finally { setSaving(false); }
  };

  const modes = Object.entries(PULL_REQUEST_MODE_LABELS).map(([value, label]) => ({ value: value as PullRequestMode, label }));
  return <div className="mx-auto w-full max-w-5xl space-y-5 p-6 pb-10">
    <Card className="border-primary/20 bg-primary/5">
      <CardHeader className="flex flex-row items-start gap-3">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-md border border-primary/20 bg-background"><Sparkles className="size-5 text-primary" /></div>
        <div><CardTitle>Publicação assistida</CardTitle><CardDescription className="mt-1 max-w-3xl leading-5">Estas regras pertencem somente a este projeto. O agente prepara commits e, quando configurado, pergunta sobre pull request e merge. Nada é publicado antes de você revisar a proposta.</CardDescription></div>
      </CardHeader>
    </Card>

    <Card>
      <CardHeader className="border-b border-border">
        <div className="flex items-center gap-2"><CardTitle>Commit</CardTitle><Badge variant="outline" className="gap-1 text-[10px] text-primary"><Sparkles className="size-3" />IA</Badge></div>
        <CardDescription>Defina como o agente deve revisar, validar, separar arquivos e escrever a mensagem de commit.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-2 pt-5">
        <Label htmlFor="project-publish-prompt">Instrução de publicação</Label>
        <Textarea id="project-publish-prompt" aria-label="Instrução de publicação" value={draft.publishPrompt} maxLength={16_000} disabled={saving} onChange={event => setDraft(current => current ? { ...current, publishPrompt: event.target.value } : current)} spellCheck={false} autoCorrect="off" autoCapitalize="none" className="min-h-40 resize-y font-mono text-xs leading-5" />
        <p className="text-[11px] text-muted-foreground">O Jarvis combina esta instrução com o estado real do Git. A proposta final sempre mostra os arquivos e a mensagem antes do commit.</p>
      </CardContent>
    </Card>

    <Card>
      <CardHeader className="border-b border-border">
        <div className="flex items-center gap-2"><GitPullRequest className="size-4 text-foreground" /><CardTitle>Pull request</CardTitle>{settings.ghAvailable ? <Badge variant="outline" className="text-[10px] text-onedark-green">gh disponível</Badge> : <Badge variant="outline" className="text-[10px] text-onedark-yellow">gh não encontrado</Badge>}</div>
        <CardDescription>Escolha se o agente deve perguntar sobre PR e merge ao preparar uma publicação.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-5 pt-5">
        {!settings.ghAvailable && <Alert className="border-onedark-yellow/25 bg-onedark-yellow/5"><AlertTriangle className="text-onedark-yellow" /><AlertTitle>GitHub CLI necessário</AlertTitle><AlertDescription>Instale o comando <span className="font-mono">gh</span> e reabra esta tela para habilitar perguntas sobre pull request. Commits locais continuam disponíveis.</AlertDescription></Alert>}
        <div className="space-y-2">
          <Label htmlFor="project-pr-mode">Comportamento ao publicar</Label>
          <Select items={modes} value={draft.prMode} disabled={saving} onValueChange={value => { if (value) setDraft(current => current ? { ...current, prMode: value } : current); }}>
            <SelectTrigger id="project-pr-mode" aria-label="Comportamento de pull request" className="w-full cursor-pointer"><SelectValue>{PULL_REQUEST_MODE_LABELS[draft.prMode]}</SelectValue></SelectTrigger>
            <SelectContent>{modes.map(mode => <SelectItem key={mode.value} value={mode.value} disabled={!settings.ghAvailable && mode.value !== "disabled"} className="cursor-pointer">{mode.label}</SelectItem>)}</SelectContent>
          </Select>
          <p className="text-[11px] text-muted-foreground">A opção de merge exige uma escolha própria no <span className="font-mono">ask_user</span>; aprovar a criação da PR não autoriza o merge.</p>
        </div>
        {draft.prMode !== "disabled" && <div className="space-y-2">
          <Label htmlFor="project-pr-prompt">Instrução e template da PR</Label>
          <Textarea id="project-pr-prompt" aria-label="Instrução e template da PR" value={draft.prPrompt} maxLength={16_000} disabled={saving} onChange={event => setDraft(current => current ? { ...current, prPrompt: event.target.value } : current)} spellCheck={false} autoCorrect="off" autoCapitalize="none" className="min-h-48 resize-y font-mono text-xs leading-5" />
        </div>}
      </CardContent>
    </Card>

    <div className="sticky bottom-0 flex justify-end border-t border-border bg-background/95 py-4 backdrop-blur">
      <Button className="cursor-pointer gap-2" disabled={saving || !changed || !draft.publishPrompt.trim() || (draft.prMode !== "disabled" && !draft.prPrompt.trim())} onClick={() => { void save(); }}><Save className="size-4" />{saving ? "Salvando…" : "Salvar opções"}</Button>
    </div>
  </div>;
}
