import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderCog, FolderOpen, Save } from "lucide-react";
import { toast } from "sonner";
import { AppearancePicker } from "@/components/settings/workflow/AppearancePicker";
import { WorkflowIdentityIcon } from "@/components/agents/WorkflowIdentityIcon";
import { PROJECT_ICON_KEYS, WORKFLOW_COLORS } from "@/components/agents/workflow-appearance";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/TextInput";
import {
  appearanceOfProject,
  libraryError,
  type Project,
  type ProjectUpdate,
} from "@/core/library";
import type { LibraryController } from "@/hooks/use-library";

type ProjectUpdater = Pick<
  LibraryController,
  "pending" | "error" | "clearError" | "updateProject"
>;

function draftOf(project: Project): ProjectUpdate {
  return {
    name: project.name,
    path: project.path,
    ...appearanceOfProject(project),
  };
}

function validName(value: string) {
  const name = value.trim();
  return (
    name.length > 0 &&
    [...name].length <= 120 &&
    ![...name].some((character) => {
      const code = character.charCodeAt(0);
      return code < 32 || (code >= 127 && code <= 159);
    })
  );
}

export function ProjectIdentitySettings({
  project,
  updater,
}: {
  project: Project;
  updater: ProjectUpdater;
}) {
  const [draft, setDraft] = useState(() => draftOf(project));
  const [selecting, setSelecting] = useState(false);

  const changed = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(draftOf(project)),
    [draft, project],
  );
  const accent = WORKFLOW_COLORS[draft.color].value;

  const chooseDirectory = async () => {
    if (selecting || updater.pending) return;
    setSelecting(true);
    updater.clearError();
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: draft.path,
        title: "Selecionar pasta do projeto",
      });
      if (typeof selected === "string") {
        setDraft((current) => ({ ...current, path: selected }));
      }
    } catch (cause) {
      toast.error(
        libraryError(cause, "Não foi possível abrir o seletor de pastas."),
      );
    } finally {
      setSelecting(false);
    }
  };

  const save = async () => {
    if (!changed || updater.pending || !validName(draft.name)) return;
    updater.clearError();
    await updater.updateProject(project.id, {
      ...draft,
      name: draft.name.trim(),
    });
  };

  return (
    <Card>
      <CardHeader className="border-b border-border">
        <div className="flex items-start gap-3">
          <span
            className="flex size-10 shrink-0 items-center justify-center rounded-md border bg-sidebar shadow-[inset_0_1px_0_rgb(255_255_255/0.04)]"
            style={{
              borderColor: `color-mix(in srgb, ${accent} 30%, transparent)`,
              backgroundColor: `color-mix(in srgb, ${accent} 8%, var(--sidebar))`,
            }}
          >
            <WorkflowIdentityIcon appearance={draft} className="size-5" />
          </span>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <FolderCog className="size-4 text-muted-foreground" />
              <CardTitle>Identidade do projeto</CardTitle>
            </div>
            <CardDescription className="mt-1 max-w-3xl leading-5">
              Ajuste o nome, a pasta vinculada e a aparência usada na barra
              lateral. Alterar a pasta não move arquivos no disco.
            </CardDescription>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-5 pt-5">
        {updater.error && (
          <Alert variant="destructive">
            <FolderCog />
            <AlertTitle>Não foi possível atualizar o projeto</AlertTitle>
            <AlertDescription>{updater.error}</AlertDescription>
          </Alert>
        )}
        <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.8fr)]">
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="project-name">Nome</Label>
              <Input
                id="project-name"
                aria-label="Nome do projeto"
                value={draft.name}
                maxLength={120}
                disabled={updater.pending}
                onChange={(event) => {
                  updater.clearError();
                  setDraft((current) => ({
                    ...current,
                    name: event.target.value,
                  }));
                }}
              />
              <p className="text-[11px] text-muted-foreground">
                Este nome identifica o projeto na navegação e nas notificações.
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="project-directory">Pasta</Label>
              <div className="flex gap-2">
                <Input
                  id="project-directory"
                  aria-label="Pasta do projeto"
                  value={draft.path}
                  readOnly
                  className="min-w-0 flex-1 font-mono text-xs"
                />
                <Button
                  type="button"
                  variant="outline"
                  className="shrink-0 cursor-pointer gap-2"
                  disabled={selecting || updater.pending}
                  onClick={() => {
                    void chooseDirectory();
                  }}
                >
                  <FolderOpen className="size-4" />
                  {selecting ? "Selecionando…" : "Alterar"}
                </Button>
              </div>
              <p className="text-[11px] text-muted-foreground">
                O Jarvis passa a trabalhar na nova pasta e mantém o histórico
                das conversas deste projeto.
              </p>
            </div>
          </div>
          <AppearancePicker
            value={draft}
            icons={PROJECT_ICON_KEYS}
            disabled={updater.pending}
            onChange={(appearance) => {
              updater.clearError();
              setDraft((current) => ({ ...current, ...appearance }));
            }}
          />
        </div>
        <div className="flex justify-end border-t border-border pt-4">
          <Button
            type="button"
            className="cursor-pointer gap-2"
            disabled={
              updater.pending ||
              !changed ||
              !validName(draft.name) ||
              !draft.path
            }
            onClick={() => {
              void save();
            }}
          >
            <Save className="size-4" />
            {updater.pending ? "Salvando…" : "Salvar projeto"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
