import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Tabs as OptionsTabs } from "@base-ui/react/tabs";
import { AlertTriangle, BookOpen, Brain, FolderCog, FolderGit2, GitCommitHorizontal, Save, ShieldCheck, Sparkles } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { libraryError, type Project } from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";
import { useIsMobile } from "@/hooks/use-mobile";
import { publicationSettingsSchema, type PublicationSettings } from "@/core/publication";
import { ProjectRepositoriesSettings } from "./ProjectRepositoriesSettings";
import { ExecutionGrantsSettings } from "./ExecutionGrantsSettings";
import { ProjectIdentitySettings } from "./ProjectIdentitySettings";
import { ProjectKnowledgeSettings } from "./ProjectKnowledgeSettings";
import { ProjectLearningSettings } from "./ProjectLearningSettings";

const SECTIONS = [
  { value: "general", label: "Geral", Icon: FolderCog, description: "Nome, pasta e aparência do projeto na barra lateral." },
  { value: "knowledge", label: "Conhecimento", Icon: BookOpen, description: "Produto, arquitetura, regras e design que orientam os agentes." },
  { value: "learning", label: "Aprendizados", Icon: Brain, description: "Lembretes e preferências aprendidos com o seu feedback." },
  { value: "repositories", label: "Repositórios", Icon: FolderGit2, description: "Pastas Git e branches de referência usadas pelos agentes." },
  { value: "commit", label: "Commit", Icon: GitCommitHorizontal, description: "Instruções do projeto para revisar mudanças e preparar commits." },
  { value: "execution", label: "Autorizações", Icon: ShieldCheck, description: "Autorizações de execução reutilizáveis neste projeto." },
] as const;

export function ProjectOptions({ project, projectUpdater }: { project: Project; projectUpdater: Pick<LibraryController, "pending" | "error" | "clearError" | "updateProject"> }) {
  const [active, setActive] = useState("general");
  const [visited, setVisited] = useState(["general"]);
  const compact = useIsMobile();
  const selected = SECTIONS.find(section => section.value === active) ?? SECTIONS[0];
  const panels = {
    general: <ProjectIdentitySettings key={`${project.id}:${project.name}:${project.path}:${project.icon ?? ""}:${project.color ?? ""}`} project={project} updater={projectUpdater} />,
    knowledge: <ProjectKnowledgeSettings key={`${project.id}:${project.path}`} projectId={project.id} />,
    learning: <ProjectLearningSettings key={`${project.id}:${project.path}`} projectId={project.id} />,
    repositories: <ProjectRepositoriesSettings key={`${project.id}:${project.path}`} projectId={project.id} projectPath={project.path} />,
    commit: <ProjectCommitSettings key={project.id} projectId={project.id} />,
    execution: <ExecutionGrantsSettings key={project.id} projectId={project.id} />,
  };

  return <OptionsTabs.Root orientation="vertical" value={active} onValueChange={value => {
    if (typeof value !== "string") return;
    setActive(value);
    setVisited(current => current.includes(value) ? current : [...current, value]);
  }} className="flex h-full min-h-0 min-w-0 gap-0 overflow-hidden">
    <nav aria-label="Seções das opções do projeto" className="settings-navigation w-14 shrink-0 overflow-y-auto border-r border-border bg-sidebar p-2 md:w-48 md:p-3">
      <TabsList aria-label="Opções do projeto" className="h-auto w-full flex-col items-stretch justify-start gap-1 rounded-none bg-transparent p-0 group-data-horizontal/tabs:h-auto">
        {SECTIONS.map(section => <Hint key={section.value} content={section.label} disabled={!compact} side="right"><TabsTrigger value={section.value} className="h-10 flex-none cursor-pointer justify-center gap-2.5 px-2 text-xs md:justify-start">
          <section.Icon aria-hidden="true" className="size-4" />
          <span className="sr-only min-w-0 flex-1 text-left md:not-sr-only">{section.label}</span>
        </TabsTrigger></Hint>)}
      </TabsList>
    </nav>
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="shrink-0 border-b border-border px-4 py-4 md:px-6">
        <h2 className="text-sm font-medium">{selected.label}</h2>
        <p className="mt-1 text-xs text-muted-foreground">{selected.description}</p>
      </header>
      {SECTIONS.map(section => <TabsContent key={section.value} value={section.value} keepMounted={visited.includes(section.value)} className="m-0 min-h-0 min-w-0 overflow-y-auto p-4 md:p-6">
        <div className="mx-auto w-full max-w-4xl">{panels[section.value]}</div>
      </TabsContent>)}
    </div>
  </OptionsTabs.Root>;
}

type Draft = Pick<PublicationSettings, "publishPrompt" | "prMode" | "prPrompt">;

function draftOf(settings: PublicationSettings): Draft {
  return { publishPrompt: settings.publishPrompt, prMode: settings.prMode, prPrompt: settings.prPrompt };
}

function OptionsSkeleton() {
  return <div role="status" aria-label="Carregando instruções de commit" className="space-y-4"><Skeleton className="h-24 w-full" /><Skeleton className="h-80 w-full" /></div>;
}

function ProjectCommitSettings({ projectId }: { projectId: string }) {
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
  if (error) return <Alert variant="destructive"><AlertTriangle /><AlertTitle>Opções de publicação indisponíveis</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>;
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

  return <div className="space-y-5">
    <Card>
      <CardHeader className="border-b border-border">
        <div className="flex items-center gap-2"><CardTitle>Commit</CardTitle><Badge variant="outline" className="gap-1 text-[10px] text-primary"><Sparkles className="size-3" />IA</Badge></div>
        <CardDescription>Defina como o agente deve revisar, validar, separar arquivos e escrever a mensagem de commit.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-2 pt-5">
        <Label htmlFor="project-publish-prompt">Instrução de publicação</Label>
        <Textarea id="project-publish-prompt" aria-label="Instrução de publicação" value={draft.publishPrompt} maxLength={16_000} disabled={saving} onChange={event => setDraft(current => current ? { ...current, publishPrompt: event.target.value } : current)} spellCheck={false} autoCorrect="off" autoCapitalize="none" className="min-h-40 resize-y font-mono text-xs leading-5" />
        <p className="text-[11px] text-muted-foreground">O agente Github combina esta instrução com o estado real do Git e as ações autorizadas na conversa.</p>
      </CardContent>
    </Card>

    <div className="sticky bottom-0 flex justify-end border-t border-border bg-background/95 py-4 backdrop-blur">
      <Button className="cursor-pointer gap-2" disabled={saving || !changed || !draft.publishPrompt.trim()} onClick={() => { void save(); }}><Save className="size-4" />{saving ? "Salvando…" : "Salvar opções"}</Button>
    </div>
  </div>;
}
