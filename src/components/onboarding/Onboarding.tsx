import { lazy, Suspense, useState } from "react";
import { ArrowLeft, ArrowRight, Check, Code2, FolderPlus, Layers, Network } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/TextInput";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { CorePanel } from "@/components/settings/CoreSettings";
import { BackupSettings } from "@/components/settings/BackupSettings";
import { useCore } from "@/hooks/use-core";
import type { ProviderAccount } from "@/core/provider-accounts";
import { OptionalToolsStep } from "./OptionalToolsStep";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { libraryError } from "@/core/library";
import { claudeModels, executionChoice, executorOf, type ExecutionSelection } from "@/core/executors";
import { useClaudeRuntime } from "@/hooks/use-claude-runtime";
import { ExecutorModelPicker } from "@/components/chat/ExecutorModelPicker";
import { accountGroups } from "@/components/settings/workflow/workflow-models";

const Providers = lazy(() => import("@/components/settings/SettingsDialog").then(module => ({ default: module.SettingsDialog })));
const STEPS = ["Boas-vindas", "Core", "Opcionais", "Provedores", "Seu espaço"];
const ignoreOpen = () => {};

export function Onboarding({ saving, onComplete }: { saving: boolean; onComplete: (workspaceName: string) => Promise<void> }) {
  const [step, setStep] = useState(0);
  const [workspace, setWorkspace] = useState("");
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [providerBusy, setProviderBusy] = useState(false);
  const [backupRestored, setBackupRestored] = useState(false);
  const [optionalBusy, setOptionalBusy] = useState(true);
  const [selection, setSelection] = useState<ExecutionSelection | null>(null);
  const [finishing, setFinishing] = useState(false);
  const claude = useClaudeRuntime(step >= 3);
  const core = useCore();
  const groups = accountGroups(accounts);
  const nativeModels = groups.flatMap(group => group.models);
  const cliModels = claudeModels(claude.data);
  const first = nativeModels[0] ?? cliModels[0];
  const choice = selection ?? (first ? { executor: nativeModels.length ? "jarvis" as const : "claude" as const, model: first.value, reasoning: first.defaultReasoningLevel ?? first.reasoningLevels[0] ?? null } : null);
  const selectedModel = (executorOf(choice) === "claude" ? cliModels : nativeModels).find(model => model.value === choice?.model);
  const providerReady = Boolean(selectedModel && (!choice?.reasoning || selectedModel.reasoningLevels.includes(choice.reasoning)) && (executorOf(choice) !== "claude" || !claude.loading && !claude.error && claude.data?.installed && claude.data.authenticated));
  const blocked = saving || finishing || (step === 1 && (!core.snapshot?.ready || core.busy)) || (step === 2 && optionalBusy) || (step === 3 && (!providerReady || providerBusy)) || (step === 4 && (!core.snapshot?.ready || !providerReady));
  const navigatingBusy = saving || finishing || core.busy || (step === 2 && optionalBusy) || (step === 3 && providerBusy);
  const complete = async () => {
    if (blocked) return;
    setFinishing(true);
    try {
      if (choice) await invoke("set_agent_model", { flow: "standard", role: "builder", choice: executionChoice(choice) });
      await onComplete(workspace);
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível concluir a configuração.")); }
    finally { setFinishing(false); }
  };
  return <main className="flex min-h-0 flex-1 items-center justify-center p-4 sm:p-6">
    <Card className="instrument-panel flex max-h-full w-full max-w-5xl flex-col gap-0 overflow-hidden py-0">
      <CardHeader className="shrink-0 gap-5 border-b border-border px-6 py-5">
        <div className="flex items-center gap-3"><JarvisLogo className="size-7" /><span className="micro-label text-muted-foreground">Primeiros passos</span></div>
        <ol aria-label="Etapas da configuração" className="grid grid-cols-5 gap-2">{STEPS.map((label, index) => <li key={label} aria-current={step === index ? "step" : undefined} className={`flex min-w-0 items-center gap-2 border-t-2 pt-3 text-xs ${step === index ? "border-primary font-medium text-primary" : step > index ? "border-onedark-green/60 text-onedark-green" : "border-border text-muted-foreground"}`}><span className="shrink-0 font-mono text-[10px]">{index < step ? <Check className="size-3" /> : `0${index + 1}`}</span><span className="truncate">{label}</span></li>)}</ol>
      </CardHeader>
      <CardContent key={step} className="min-h-0 overflow-y-auto overscroll-contain px-6 py-6 motion-safe:animate-in motion-safe:fade-in motion-safe:duration-200">
        {step === 0 && <div className="space-y-7 py-4"><div className="max-w-xl space-y-3"><h1 className="text-2xl font-medium tracking-tight">Bem-vindo ao Jarvis</h1><p className="text-sm leading-6 text-muted-foreground">Transforme ideias em projetos com agentes de IA. Planeje, investigue, construa e revise código no seu próprio ambiente.</p></div><div className="grid gap-4 sm:grid-cols-3">{[{ icon: Code2, title: "Do plano ao código", text: "Agentes especializados trabalham em conjunto no seu projeto." }, { icon: Network, title: "Seus modelos", text: "Conecte provedores e escolha o modelo ideal para cada agente." }, { icon: Layers, title: "Tudo organizado", text: "Workspaces, projetos e conversas com histórico e tarefas persistentes." }].map(({ icon: Icon, title, text }) => <div key={title} className="space-y-3 border-t border-border pt-4"><Icon className="size-5 text-primary" /><h2 className="text-sm font-medium">{title}</h2><p className="text-xs leading-5 text-muted-foreground">{text}</p></div>)}</div><BackupSettings accounts={accounts} restoreOnly onRestored={() => setBackupRestored(true)} />{backupRestored && <p role="status" className="text-xs text-onedark-green">Backup restaurado. Conecte os provedores desta instalação para continuar.</p>}</div>}
        {step === 1 && <div className="space-y-5"><div className="space-y-2"><h1 className="text-2xl font-medium tracking-tight">Prepare o essencial</h1><p className="text-sm text-muted-foreground">Cinco componentes locais deixam o Jarvis pronto. Context7 é opcional e pode ser configurado depois.</p></div><CorePanel core={core} setup /></div>}
        {step === 2 && <OptionalToolsStep onBusyChange={setOptionalBusy} />}
        {step === 3 && <div className="flex flex-col gap-5"><div className="flex flex-col gap-2"><h1 className="text-2xl font-medium tracking-tight">Conecte sua inteligência</h1><p className="text-sm text-muted-foreground">Configure suas contas ou a instalação local do Claude Code e escolha um modelo para começar.</p></div><Suspense fallback={<CardsSkeleton label="Carregando provedores" columns />}><Providers open embeddedProviders onOpenChange={ignoreOpen} onAccountsChange={setAccounts} onBusyChange={setProviderBusy} /></Suspense><div className="flex flex-col gap-2"><p className="micro-label text-muted-foreground">Modelo inicial</p><div className="flex items-center gap-1"><ExecutorModelPicker selection={choice} modelGroups={groups} showProviderIdentity onSelect={setSelection} /></div></div></div>}
        {step === 4 && <div className="mx-auto max-w-lg space-y-6 py-4"><div className="flex size-12 items-center justify-center rounded-lg border border-onedark-green/25 bg-onedark-green/10 text-onedark-green"><Check className="size-6" /></div><div className="space-y-2"><h1 className="text-2xl font-medium tracking-tight">Tudo pronto para começar</h1><p className="text-sm leading-6 text-muted-foreground">Dê um nome ao seu workspace. Depois, adicione a pasta do seu primeiro projeto.</p></div><div className="space-y-2"><Label htmlFor="first-workspace">Workspace padrão</Label><Input id="first-workspace" placeholder="Pessoal" value={workspace} maxLength={120} disabled={saving || finishing} onChange={event => setWorkspace(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !blocked) void complete(); }} /></div></div>}
      </CardContent>
      <CardFooter className="shrink-0 justify-between border-t border-border bg-sidebar/40 px-6 py-4"><div>{step > 0 && <Button variant="ghost" disabled={navigatingBusy} onClick={() => setStep(step - 1)}><ArrowLeft className="size-4" />Voltar</Button>}</div><Button disabled={blocked} aria-busy={saving || finishing || (step === 2 && optionalBusy)} onClick={() => { if (step < 4) setStep(step + 1); else void complete(); }}>{step === 4 ? <><FolderPlus className="size-4" />{saving || finishing ? "Preparando…" : "Começar"}</> : <>Avançar<ArrowRight className="size-4" /></>}</Button></CardFooter>
    </Card>
  </main>;
}
