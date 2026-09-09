import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowUpCircle, BookOpen, Boxes, Check, CircleAlert, Cpu, Sparkles } from "lucide-react";
import { toast } from "sonner";
import type { BootstrapResourcesController } from "@/core/bootstrap-context";
import { useBootstrapResources } from "@/hooks/use-bootstrap-resources";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { skillUpdateSchema, skillError } from "@/core/skills";
import { useCore } from "@/hooks/use-core";
import { CoreInstallProgress } from "@/components/core/CoreInstallProgress";

type Operation = {
  completed: number;
  total: number;
  label: string;
};

export function ResourceUpdates() {
  const bootstrap = useBootstrapResources();
  if (!bootstrap) return null;
  return <ResourceUpdatesContent bootstrap={bootstrap} />;
}

function ResourceUpdatesContent({ bootstrap }: { bootstrap: BootstrapResourcesController }) {
  const updateBootstrapSkills = bootstrap.updateSkills;
  const core = useCore();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const [completed, setCompleted] = useState(false);

  const coreUpdates = useMemo(
    () => core.snapshot?.items.filter((item) => item.updateAvailable) ?? [],
    [core.snapshot],
  );
  const skillUpdates = useMemo(
    () => bootstrap?.resources.skills?.skills.filter((skill) => skill.updateAvailable) ?? [],
    [bootstrap?.resources.skills],
  );
  const updateCount = coreUpdates.length + skillUpdates.length;
  const overallPercent = operation?.total
    ? Math.round(operation.completed / operation.total * 100)
    : 0;

  if (updateCount === 0 && !open) return null;

  async function installAll() {
    if (busy || updateCount === 0) return;
    const selectedCore = coreUpdates.map((item) => item.id);
    const selectedSkills = skillUpdates.map((skill) => skill.id);
    const total = selectedCore.length + selectedSkills.length;
    let done = 0;
    const failures: string[] = [];
    setBusy(true);
    setCompleted(false);
    setErrors([]);
    setOperation({ completed: 0, total, label: "Preparando atualizações" });
    try {
      if (selectedCore.length > 0) {
        setOperation({ completed: done, total, label: "Atualizando ferramentas do Core" });
        const successful = await core.install(selectedCore, { silent: true });
        if (successful) done += selectedCore.length;
        else failures.push("Uma ou mais ferramentas do Core não puderam ser atualizadas.");
        setOperation({ completed: done, total, label: selectedSkills.length ? "Preparando skills" : "Finalizando" });
      }

      if (selectedSkills.length > 0) {
        setOperation({ completed: done, total, label: "Atualizando skills do Marketplace" });
        try {
          const result = skillUpdateSchema.parse(await invoke("update_skills", { ids: selectedSkills }));
          updateBootstrapSkills(result.snapshot, true);
          done += result.updated;
          failures.push(...result.errors);
        } catch (cause) {
          failures.push(skillError(cause));
        }
      }

      setOperation({ completed: done, total, label: failures.length ? "Atualização concluída com ressalvas" : "Recursos atualizados" });
      setErrors(failures);
      setCompleted(failures.length === 0 && done === total);
      if (failures.length === 0 && done === total) toast.success("Recursos atualizados");
    } finally {
      setBusy(false);
    }
  }

  return <Dialog open={open} onOpenChange={(next) => { if (!busy) { setOpen(next); if (next) { setErrors([]); setCompleted(false); setOperation(null); } } }}>
    <DialogTrigger render={<Button variant="ghost" size="sm" />} className="h-6 shrink-0 cursor-pointer gap-1.5 rounded-sm px-1.5 font-mono text-[10px] text-onedark-yellow" aria-label={`${updateCount} ${updateCount === 1 ? "atualização de recurso disponível" : "atualizações de recursos disponíveis"}`}>
      <Boxes className="size-3" />
      <span>Atualizações de recursos</span>
      <Badge variant="outline" className="border-onedark-yellow/25 bg-onedark-yellow/5 text-onedark-yellow">{updateCount}</Badge>
    </DialogTrigger>
    <DialogContent className="dark flex max-h-[82vh] flex-col overflow-hidden sm:max-w-xl" showCloseButton={!busy}>
      <DialogHeader className="shrink-0">
        <DialogTitle className="flex items-center gap-2 text-base"><Sparkles className="size-4 text-onedark-yellow" />Atualizações de recursos</DialogTitle>
        <DialogDescription>Atualize as ferramentas do Core e as skills instaladas pelo Marketplace.</DialogDescription>
      </DialogHeader>

      <div className="min-h-0 space-y-5 overflow-y-auto pr-1">
        {coreUpdates.length > 0 && <section aria-labelledby="resource-core-title" className="space-y-2">
          <div className="flex items-center gap-2"><Cpu className="size-3.5 text-onedark-cyan" /><h3 id="resource-core-title" className="micro-label text-muted-foreground">Core</h3><Badge variant="secondary">{coreUpdates.length}</Badge></div>
          {coreUpdates.map((item) => <Card key={item.id} className="gap-0 p-3">
            <div className="flex min-w-0 items-center gap-3">
              <span className="flex size-8 shrink-0 items-center justify-center rounded-md border border-onedark-cyan/25 bg-onedark-cyan/5 text-onedark-cyan"><Cpu className="size-4" /></span>
              <span className="min-w-0 flex-1"><span className="block truncate text-xs font-medium">{item.name}</span><span className="mt-1 block truncate font-mono text-[10px] text-muted-foreground">{item.installedVersion ?? "versão local"} → {item.latestVersion ?? "nova versão"}</span></span>
              {busy && item.stage && <Badge variant="outline" className="border-onedark-cyan/25 text-onedark-cyan">Em andamento</Badge>}
            </div>
            {busy && item.stage && <CoreInstallProgress item={item} operation="Atualização" />}
          </Card>)}
        </section>}

        {skillUpdates.length > 0 && <section aria-labelledby="resource-skills-title" className="space-y-2">
          <div className="flex items-center gap-2"><BookOpen className="size-3.5 text-onedark-purple" /><h3 id="resource-skills-title" className="micro-label text-muted-foreground">Skills do Marketplace</h3><Badge variant="secondary">{skillUpdates.length}</Badge></div>
          {skillUpdates.map((skill) => <Card key={skill.id} className="gap-0 p-3"><div className="flex min-w-0 items-center gap-3"><span className="flex size-8 shrink-0 items-center justify-center rounded-md border border-onedark-purple/25 bg-onedark-purple/5 text-onedark-purple"><BookOpen className="size-4" /></span><span className="min-w-0 flex-1"><span className="block truncate text-xs font-medium">{skill.name}</span><span className="mt-1 block truncate font-mono text-[10px] text-muted-foreground">{skill.source ?? "Marketplace"} · nova revisão disponível</span></span></div></Card>)}
        </section>}

        {updateCount === 0 && completed && <Alert className="border-onedark-green/25 bg-onedark-green/5 text-onedark-green"><Check /><AlertDescription className="text-onedark-green">Todas as atualizações foram instaladas.</AlertDescription></Alert>}
        {errors.length > 0 && <Alert variant="destructive"><CircleAlert /><AlertDescription><ul className="space-y-1">{errors.map((error, index) => <li key={`${error}/${index}`}>{error}</li>)}</ul></AlertDescription></Alert>}

        {operation && <div role="status" aria-live="polite" className="space-y-2 rounded-md border border-border bg-secondary/25 p-3">
          <div className="flex items-center justify-between gap-3 text-xs"><span>{operation.label}</span><span className="font-mono tabular-nums text-onedark-yellow">{overallPercent}%</span></div>
          <Progress value={overallPercent} aria-label={operation.label} className="[&_[data-slot=progress-track]]:h-1.5 [&_[data-slot=progress-indicator]]:bg-onedark-yellow" />
          <p className="font-mono text-[9px] text-muted-foreground">{operation.completed}/{operation.total} recursos concluídos</p>
        </div>}
      </div>

      <DialogFooter className="shrink-0">
        {updateCount > 0 && <Button disabled={busy} onClick={() => void installAll()} className="cursor-pointer"><ArrowUpCircle data-icon="inline-start" />{busy ? "Instalando atualizações…" : `Instalar ${updateCount === 1 ? "atualização" : `${updateCount} atualizações`}`}</Button>}
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
