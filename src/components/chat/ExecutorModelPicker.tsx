import { useState, type ComponentProps } from "react";
import { CircleHelp } from "lucide-react";
import { executorOf, claudeModels, museModels } from "@/core/executors";
import { useClaudeRuntime } from "@/hooks/use-claude-runtime";
import { useMuseRuntime } from "@/hooks/use-muse-runtime";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { ClaudeProviderDialog } from "@/components/settings/ClaudeProviderCard";
import { MuseProviderDialog } from "@/components/settings/MuseProviderCard";
import { ModelPicker, type ProviderModelGroup } from "./ModelPicker";

export function ExecutorModelPicker(props: ComponentProps<typeof ModelPicker>) {
  const runtime = useClaudeRuntime();
  const muse = useMuseRuntime();
  const [details, setDetails] = useState(false);
  const models = claudeModels(runtime.data);
  // Keep offline agent configuration possible; execution still requires CLI login.
  const offlineModels = !runtime.loading && !runtime.data?.installed && !runtime.error && !runtime.data?.preferences?.disabledModels.includes("default")
    ? [{ value: "default", label: "Padrão do Claude Code", reasoningLevels: [], defaultReasoningLevel: null }]
    : [];
  const groups: ProviderModelGroup[] = [
    ...props.modelGroups.filter(group => executorOf(group) === "jarvis"),
    ...(runtime.data?.preferences?.enabled === false ? [] : [{ provider: "Claude Code", executor: "claude" as const, models: runtime.data?.installed ? models : offlineModels, emptyMessage: runtime.loading ? "Consultando Claude Code…" : "Configure os modelos em Configurações → Provedores → Claude Code." }]),
    ...(muse.data?.preferences?.enabled === false ? [] : [{ provider: "Muse Code", executor: "muse" as const, models: museModels(muse.data), emptyMessage: muse.loading ? "Consultando Muse Code…" : "Conecte a assinatura em Configurações → Provedores → Muse Code." }]),
  ];
  const claudeSelected = executorOf(props.selection) === "claude";
  const museSelected = executorOf(props.selection) === "muse";
  const selectedRuntime = museSelected ? muse : claudeSelected ? runtime : null;
  return <>
    <ModelPicker {...props} modelGroups={groups} onSelect={next => props.onSelect({ ...next, executor: executorOf(next) })} onRefresh={selectedRuntime ? () => { void selectedRuntime.refresh(); } : props.onRefresh} refreshing={selectedRuntime ? selectedRuntime.loading : props.refreshing} />
    {claudeSelected && <Hint content="Configurar instalação, conta e modelos do Claude Code"><Button type="button" size="icon-sm" variant="ghost" aria-label="Status e configuração do Claude Code" className="shrink-0 cursor-pointer" onClick={() => setDetails(true)}><CircleHelp /></Button></Hint>}
    <ClaudeProviderDialog open={claudeSelected && details} onOpenChange={setDetails} />
    {museSelected && <Hint content="Configurar instalação, assinatura e modelos do Muse Code"><Button type="button" size="icon-sm" variant="ghost" aria-label="Status e configuração do Muse Code" className="shrink-0 cursor-pointer" onClick={() => setDetails(true)}><CircleHelp /></Button></Hint>}
    <MuseProviderDialog open={museSelected && details} onOpenChange={setDetails} />
  </>;
}
