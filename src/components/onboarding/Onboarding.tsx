import { lazy, Suspense, useState } from "react";
import { ArrowLeft, ArrowRight, Check, Code2, FolderPlus, Layers, Network } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/TextInput";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { CorePanel } from "@/components/settings/CoreSettings";
import { useCore } from "@/hooks/use-core";
import type { ProviderAccount } from "@/core/provider-accounts";
import { OptionalToolsStep } from "./OptionalToolsStep";

const Providers = lazy(() => import("@/components/settings/SettingsDialog").then(module => ({ default: module.SettingsDialog })));
const STEPS = ["Boas-vindas", "Core", "Opcionais", "Provedores", "Seu espaço"];
const ignoreOpen = () => {};

export function Onboarding({ saving, onComplete }: { saving: boolean; onComplete: (workspaceName: string) => Promise<void> }) {
  const [step, setStep] = useState(0);
  const [workspace, setWorkspace] = useState("");
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [providerBusy, setProviderBusy] = useState(false);
  const [optionalBusy, setOptionalBusy] = useState(true);
  const core = useCore();
  const providerReady = accounts.some(account => account.enabled && account.modelsAvailable && account.models.length > 0);
  const blocked = saving || (step === 1 && (!core.snapshot?.ready || core.busy)) || (step === 2 && optionalBusy) || (step === 3 && (!providerReady || providerBusy)) || (step === 4 && (!core.snapshot?.ready || !providerReady));
  const navigatingBusy = saving || core.busy || (step === 2 && optionalBusy) || (step === 3 && providerBusy);
  return <main className="flex min-h-0 flex-1 items-center justify-center p-4 sm:p-6">
    <Card className="instrument-panel flex max-h-full w-full max-w-5xl flex-col gap-0 overflow-hidden py-0">
      <CardHeader className="shrink-0 gap-5 border-b border-border px-6 py-5">
        <div className="flex items-center gap-3"><JarvisLogo className="size-7" /><span className="micro-label text-muted-foreground">Primeiros passos</span></div>
        <ol aria-label="Etapas da configuração" className="grid grid-cols-5 gap-2">{STEPS.map((label, index) => <li key={label} aria-current={step === index ? "step" : undefined} className={`flex min-w-0 items-center gap-2 border-t-2 pt-3 text-xs ${step === index ? "border-primary font-medium text-primary" : step > index ? "border-onedark-green/60 text-onedark-green" : "border-border text-muted-foreground"}`}><span className="shrink-0 font-mono text-[10px]">{index < step ? <Check className="size-3" /> : `0${index + 1}`}</span><span className="truncate">{label}</span></li>)}</ol>
      </CardHeader>
      <CardContent key={step} className="min-h-0 overflow-y-auto overscroll-contain px-6 py-6 motion-safe:animate-in motion-safe:fade-in motion-safe:duration-200">
        {step === 0 && <div className="space-y-7 py-4"><div className="max-w-xl space-y-3"><h1 className="text-2xl font-medium tracking-tight">Bem-vindo ao Jarvis</h1><p className="text-sm leading-6 text-muted-foreground">Transforme ideias em projetos com agentes de IA. Planeje, investigue, construa e revise código no seu próprio ambiente.</p></div><div className="grid gap-4 sm:grid-cols-3">{[{ icon: Code2, title: "Do plano ao código", text: "Agentes especializados trabalham em conjunto no seu projeto." }, { icon: Network, title: "Seus modelos", text: "Conecte provedores e escolha o modelo ideal para cada agente." }, { icon: Layers, title: "Tudo organizado", text: "Workspaces, projetos e conversas com histórico e tarefas persistentes." }].map(({ icon: Icon, title, text }) => <div key={title} className="space-y-3 border-t border-border pt-4"><Icon className="size-5 text-primary" /><h2 className="text-sm font-medium">{title}</h2><p className="text-xs leading-5 text-muted-foreground">{text}</p></div>)}</div></div>}
        {step === 1 && <div className="space-y-5"><div className="space-y-2"><h1 className="text-2xl font-medium tracking-tight">Prepare suas ferramentas</h1><p className="text-sm text-muted-foreground">Seis componentes locais em ~/.jarvis.</p></div><CorePanel core={core} setup /></div>}
        {step === 2 && <OptionalToolsStep onBusyChange={setOptionalBusy} />}
        {step === 3 && <div className="space-y-5"><div className="space-y-2"><h1 className="text-2xl font-medium tracking-tight">Conecte sua inteligência</h1><p className="text-sm text-muted-foreground">Adicione um provedor e escolha como usar Web Search e Vision.</p></div><Suspense fallback={<CardsSkeleton label="Carregando provedores" columns />}><Providers open embeddedProviders onOpenChange={ignoreOpen} onAccountsChange={setAccounts} onBusyChange={setProviderBusy} /></Suspense></div>}
        {step === 4 && <div className="mx-auto max-w-lg space-y-6 py-4"><div className="flex size-12 items-center justify-center rounded-lg border border-onedark-green/25 bg-onedark-green/10 text-onedark-green"><Check className="size-6" /></div><div className="space-y-2"><h1 className="text-2xl font-medium tracking-tight">Tudo pronto para começar</h1><p className="text-sm leading-6 text-muted-foreground">Dê um nome ao seu workspace. Depois, adicione a pasta do seu primeiro projeto.</p></div><div className="space-y-2"><Label htmlFor="first-workspace">Workspace padrão</Label><Input id="first-workspace" placeholder="Pessoal" value={workspace} maxLength={120} disabled={saving} onChange={event => setWorkspace(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !blocked) void onComplete(workspace); }} /></div></div>}
      </CardContent>
      <CardFooter className="shrink-0 justify-between border-t border-border bg-sidebar/40 px-6 py-4"><div>{step > 0 && <Button variant="ghost" disabled={navigatingBusy} onClick={() => setStep(step - 1)}><ArrowLeft className="size-4" />Voltar</Button>}</div><Button disabled={blocked} aria-busy={saving || (step === 2 && optionalBusy)} onClick={() => { if (step < 4) setStep(step + 1); else void onComplete(workspace); }}>{step === 4 ? <><FolderPlus className="size-4" />{saving ? "Preparando…" : "Começar"}</> : <>Avançar<ArrowRight className="size-4" /></>}</Button></CardFooter>
    </Card>
  </main>;
}
