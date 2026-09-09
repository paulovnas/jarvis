import { useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Archive, ArrowRight, Bot, BookOpen, CircleAlert, Download, Plug, ShieldCheck, Upload, Workflow } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import type { ProviderAccount } from "@/core/provider-accounts";
import { compatibleModels, defaultChoice, modelProblem, type ModelChoice } from "@/core/provider-references";
import {
  backupExportResultSchema,
  backupImportResultSchema,
  backupPreviewSchema,
  formatBackupSize,
  type BackupModelTarget,
  type BackupPreview,
} from "@/core/settings-backup";
import { reasoningLabel } from "@/core/reasoning";
import { ChoiceField } from "./workflow/WorkflowFields";

const UNASSIGNED = "__unassigned__";

function backupError(cause: unknown, fallback: string): string {
  if (typeof cause === "object" && cause !== null && "message" in cause && typeof cause.message === "string") return cause.message;
  if (typeof cause === "string") return cause;
  return fallback;
}

function SummaryCard({ icon: Icon, label, value }: { icon: typeof Bot; label: string; value: number }) {
  return <Card className="min-w-0 gap-2 rounded-lg p-3">
    <span className="flex items-center gap-2 text-[11px] text-muted-foreground"><Icon aria-hidden="true" className="size-3.5 text-onedark-cyan" />{label}</span>
    <strong className="font-mono text-lg font-medium tabular-nums">{value}</strong>
  </Card>;
}

function ModelMappingRow({ target, accounts, choice, busy, onChange }: { target: BackupModelTarget; accounts: ProviderAccount[]; choice?: ModelChoice; busy: boolean; onChange: (choice?: ModelChoice) => void }) {
  const available = accounts.filter(account => compatibleModels(account, target.kind).length > 0);
  const account = available.find(item => item.alias === choice?.account);
  const models = account ? compatibleModels(account, target.kind) : [];
  const model = models.find(item => item.id === choice?.model);
  return <Card aria-label={`Mapeamento: ${target.label}`} className="gap-3 rounded-lg p-4">
    <div className="flex flex-wrap items-center gap-2">
      <Bot aria-hidden="true" className="size-3.5 text-onedark-purple" />
      <span className="text-sm font-medium">{target.label}</span>
      <Badge variant="outline" className={`rounded-md ${choice ? "border-onedark-green/30 text-onedark-green" : "border-onedark-yellow/30 text-onedark-yellow"}`}>{choice ? "Modelo definido" : "Configurar depois"}</Badge>
    </div>
    <div className="grid min-w-0 gap-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1.45fr)] sm:items-center">
      <div className="min-w-0 rounded-md border border-border bg-sidebar p-3">
        <p className="micro-label mb-2 text-muted-foreground">De · Backup</p>
        <p className="truncate text-xs font-medium">{target.label}</p>
        <p className="mt-1 text-[11px] text-muted-foreground">{target.details.join(" · ")}</p>
        <p className="mt-2 font-mono text-[10px] text-muted-foreground">Modelo removido do arquivo</p>
      </div>
      <ArrowRight aria-hidden="true" className="hidden size-4 text-muted-foreground sm:block" />
      <div className="min-w-0 space-y-3">
        <p className="micro-label text-muted-foreground">Para · Neste Jarvis</p>
        <ChoiceField
          label={`Provedor para ${target.label}`}
          value={choice?.account ?? UNASSIGNED}
          disabled={busy}
          options={[{ value: UNASSIGNED, label: "Configurar depois" }, ...available.map(item => ({ value: item.alias, label: item.alias }))]}
          onChange={alias => {
            const next = available.find(item => item.alias === alias);
            const first = next ? compatibleModels(next, target.kind)[0] : undefined;
            onChange(next && first ? defaultChoice(next, first) : undefined);
          }}
        />
        {choice && <ChoiceField
          label={`Modelo para ${target.label}`}
          value={choice.model}
          disabled={busy || models.length === 0}
          options={models.map(item => ({ value: item.id, label: item.name }))}
          onChange={id => {
            const next = models.find(item => item.id === id);
            if (account && next) onChange(defaultChoice(account, next));
          }}
        />}
        {choice && model?.reasoningLevels.length ? <ChoiceField
          label={`Raciocínio para ${target.label}`}
          value={choice.reasoning ?? model.defaultReasoningLevel ?? model.reasoningLevels[0]}
          disabled={busy}
          options={model.reasoningLevels.map(level => ({ value: level, label: reasoningLabel(level) }))}
          onChange={reasoning => onChange({ ...choice, reasoning })}
        /> : null}
        {available.length === 0 && <p className="text-xs leading-relaxed text-onedark-yellow">Nenhum modelo conectado está disponível. O agente será restaurado sem modelo.</p>}
        {choice && modelProblem(choice, accounts, target.kind) && <p role="alert" className="text-xs text-destructive">{modelProblem(choice, accounts, target.kind)}</p>}
      </div>
    </div>
  </Card>;
}

export function BackupSettings({ accounts, onRestored }: { accounts: ProviderAccount[]; onRestored?: (summary: BackupPreview["summary"]) => void }) {
  const [exporting, setExporting] = useState(false);
  const [inspecting, setInspecting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [step, setStep] = useState<"review" | "models">("review");
  const [choices, setChoices] = useState<Record<string, ModelChoice>>({});
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const invalidMapping = useMemo(() => preview?.modelTargets.some(target => choices[target.id] && modelProblem(choices[target.id], accounts, target.kind)) ?? false, [accounts, choices, preview]);
  const busy = exporting || inspecting || importing;

  async function createBackup() {
    if (inFlight.current) return;
    const day = new Date().toISOString().slice(0, 10);
    const destination = await save({
      title: "Salvar backup do Jarvis",
      defaultPath: `Jarvis-backup-${day}.zip`,
      filters: [{ name: "Backup do Jarvis", extensions: ["zip"] }],
    });
    if (!destination) return;
    inFlight.current = true;
    setExporting(true);
    setError(null);
    try {
      const result = backupExportResultSchema.parse(await invoke("export_settings_backup", { path: destination }));
      toast.success("Backup criado", { description: `${formatBackupSize(result.bytes)} · ${result.path}` });
    } catch (cause) {
      const message = backupError(cause, "Não foi possível criar o backup.");
      setError(message);
      toast.error(message);
    } finally {
      inFlight.current = false;
      setExporting(false);
    }
  }

  async function chooseBackup() {
    if (inFlight.current) return;
    const selected = await open({
      title: "Escolher backup do Jarvis",
      multiple: false,
      directory: false,
      filters: [{ name: "Backup do Jarvis", extensions: ["zip"] }],
    });
    if (typeof selected !== "string") return;
    inFlight.current = true;
    setInspecting(true);
    setError(null);
    try {
      const inspected = backupPreviewSchema.parse(await invoke("inspect_settings_backup", { path: selected }));
      setSourcePath(selected);
      setPreview(inspected);
      setChoices({});
      setStep("review");
    } catch (cause) {
      const message = backupError(cause, "Não foi possível inspecionar o backup.");
      setError(message);
      toast.error(message);
    } finally {
      inFlight.current = false;
      setInspecting(false);
    }
  }

  function resetPreview() {
    setPreview(null);
    setSourcePath(null);
    setChoices({});
    setStep("review");
  }

  function closePreview() {
    if (!importing) resetPreview();
  }

  async function restore() {
    if (!preview || !sourcePath || inFlight.current || invalidMapping) return;
    inFlight.current = true;
    setImporting(true);
    setError(null);
    try {
      const result = backupImportResultSchema.parse(await invoke("import_settings_backup", {
        path: sourcePath,
        fingerprint: preview.fingerprint,
        mappings: preview.modelTargets.flatMap(target => choices[target.id] ? [{ targetId: target.id, choice: choices[target.id] }] : []),
      }));
      resetPreview();
      onRestored?.(result.summary);
      toast.success("Configurações restauradas", { description: `${result.mappedModels} ${result.mappedModels === 1 ? "modelo associado" : "modelos associados"}. Provedores e históricos foram preservados.` });
    } catch (cause) {
      const message = backupError(cause, "Não foi possível restaurar o backup.");
      setError(message);
      toast.error(message);
    } finally {
      inFlight.current = false;
      setImporting(false);
    }
  }

  return <section aria-labelledby="backup-settings-title" className="mt-6 space-y-3 border-t border-border pt-6">
    <div className="space-y-1">
      <h2 id="backup-settings-title" className="micro-label flex items-center gap-2 text-muted-foreground"><Archive aria-hidden="true" className="size-3.5" />Backup e restauração</h2>
      <p className="text-xs leading-relaxed text-muted-foreground">Proteja suas preferências, agentes, fluxos, skills e MCPs em um único arquivo portátil.</p>
    </div>
    <div className="grid gap-3 sm:grid-cols-2">
      <Card className="min-w-0 gap-4 p-4">
        <div className="flex items-start gap-3">
          <span className="rounded-md border border-onedark-green/20 bg-onedark-green/10 p-2 text-onedark-green"><Download aria-hidden="true" className="size-4" /></span>
          <div className="min-w-0 space-y-1"><h3 className="text-sm font-medium">Criar backup</h3><p className="text-[11px] leading-relaxed text-muted-foreground">Escolha onde salvar um ZIP com as configurações atuais do Jarvis.</p></div>
        </div>
        <div className="flex flex-wrap gap-1.5">{["Sistema", "Agentes e fluxos", "Skills", "MCPs"].map(item => <Badge key={item} variant="outline" className="rounded-md text-[9px] text-muted-foreground">{item}</Badge>)}</div>
        <p className="flex items-start gap-2 text-[10px] leading-relaxed text-onedark-yellow"><CircleAlert aria-hidden="true" className="mt-0.5 size-3 shrink-0" />MCPs podem incluir chaves de acesso. Guarde o ZIP em um local seguro.</p>
        <Button type="button" variant="outline" disabled={busy} onClick={() => void createBackup()} className="mt-auto w-full cursor-pointer gap-2">{exporting ? <><Spinner aria-hidden="true" />Criando backup…</> : <><Download aria-hidden="true" />Escolher destino</>}</Button>
      </Card>
      <Card className="min-w-0 gap-4 p-4">
        <div className="flex items-start gap-3">
          <span className="rounded-md border border-onedark-cyan/20 bg-onedark-cyan/10 p-2 text-onedark-cyan"><Upload aria-hidden="true" className="size-4" /></span>
          <div className="min-w-0 space-y-1"><h3 className="text-sm font-medium">Restaurar backup</h3><p className="text-[11px] leading-relaxed text-muted-foreground">Inspecione o conteúdo e associe os agentes aos modelos disponíveis antes de aplicar.</p></div>
        </div>
        <p className="flex items-start gap-2 text-[10px] leading-relaxed text-onedark-yellow"><ShieldCheck aria-hidden="true" className="mt-0.5 size-3 shrink-0" />Contas de IA, projetos e conversas permanecem nesta instalação.</p>
        <Button type="button" variant="outline" disabled={busy} onClick={() => void chooseBackup()} className="mt-auto w-full cursor-pointer gap-2">{inspecting ? <><Spinner aria-hidden="true" />Inspecionando…</> : <><Upload aria-hidden="true" />Escolher arquivo</>}</Button>
      </Card>
    </div>
    {error && !preview && <p role="alert" className="text-xs text-destructive">{error}</p>}

    <Dialog open={preview !== null} onOpenChange={open => { if (!open) closePreview(); }}>
      <DialogContent showCloseButton={!importing} className="dark flex max-h-[88dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl">
        <DialogHeader className="shrink-0 border-b border-border p-5 pr-12">
          <DialogTitle className="flex items-center gap-2"><Archive aria-hidden="true" className="size-4 text-onedark-cyan" />{step === "review" ? "Revisar backup" : "Associar modelos"}</DialogTitle>
          <DialogDescription>{step === "review" ? "Confira o conteúdo antes de substituir as configurações atuais." : "Os vínculos anteriores não estão no arquivo. Escolha modelos desta instalação ou configure depois."}</DialogDescription>
        </DialogHeader>
        {preview && <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-5">
          {step === "review" ? <>
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              <SummaryCard icon={Bot} label="Agentes" value={preview.summary.customAgents} />
              <SummaryCard icon={Workflow} label="Fluxos" value={preview.summary.customFlows} />
              <SummaryCard icon={BookOpen} label="Skills" value={preview.summary.skills} />
              <SummaryCard icon={Plug} label="MCPs" value={preview.summary.mcps} />
            </div>
            <Card className="grid gap-3 rounded-lg p-4 sm:grid-cols-3">
              <div><p className="micro-label text-muted-foreground">Criado em</p><p className="mt-1 font-mono text-xs tabular-nums">{new Date(preview.createdAt * 1000).toLocaleString("pt-BR")}</p></div>
              <div><p className="micro-label text-muted-foreground">Versão de origem</p><p className="mt-1 font-mono text-xs">Jarvis {preview.appVersion}</p></div>
              <div><p className="micro-label text-muted-foreground">Tamanho</p><p className="mt-1 font-mono text-xs tabular-nums">{formatBackupSize(preview.archiveBytes)}</p></div>
            </Card>
            <Alert className="border-onedark-yellow/30 bg-onedark-yellow/5"><CircleAlert className="text-onedark-yellow" /><AlertTitle className="text-onedark-yellow">A restauração substitui estas categorias</AlertTitle><AlertDescription className="space-y-1 text-xs leading-relaxed">{preview.warnings.map(warning => <p key={warning}>{warning}</p>)}</AlertDescription></Alert>
          </> : <div className="space-y-3">
            {preview.modelTargets.length === 0 ? <p className="rounded-lg border border-dashed p-6 text-center text-xs text-muted-foreground">Este backup não possui agentes que precisem de associação.</p> : preview.modelTargets.map(target => <ModelMappingRow key={target.id} target={target} accounts={accounts} choice={choices[target.id]} busy={importing} onChange={choice => setChoices(current => { const next = { ...current }; if (choice) next[target.id] = choice; else delete next[target.id]; return next; })} />)}
          </div>}
          {error && <Alert variant="destructive"><CircleAlert /><AlertTitle>Não foi possível restaurar</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>}
        </div>}
        <DialogFooter className="m-0 shrink-0 flex-col gap-3 border-t border-border bg-card p-5 sm:flex-row sm:items-center sm:justify-between">
          <p role="status" aria-live="polite" className="text-xs text-muted-foreground">{step === "review" ? `${preview?.summary.modelTargets ?? 0} agentes poderão receber novos modelos` : `${Object.keys(choices).length} de ${preview?.modelTargets.length ?? 0} modelos associados`}</p>
          <div className="flex justify-end gap-2">
            {step === "models" && <Button type="button" variant="outline" disabled={importing} onClick={() => { setError(null); setStep("review"); }} className="cursor-pointer">Voltar</Button>}
            <Button type="button" variant="outline" disabled={importing} onClick={() => closePreview()} className="cursor-pointer">Cancelar</Button>
            {step === "review" && preview?.modelTargets.length
              ? <Button type="button" disabled={importing} onClick={() => { setError(null); setStep("models"); }} className="cursor-pointer">Revisar modelos<ArrowRight aria-hidden="true" /></Button>
              : <Button type="button" disabled={importing || invalidMapping} onClick={() => void restore()} className="cursor-pointer">{importing ? <><Spinner aria-hidden="true" />Restaurando…</> : "Restaurar configurações"}</Button>}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </section>;
}
