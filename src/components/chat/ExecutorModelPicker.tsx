import { useState, type ComponentProps } from "react";
import { CircleHelp } from "lucide-react";
import { executorOf, claudeModels } from "@/core/executors";
import { useClaudeRuntime } from "@/hooks/use-claude-runtime";
import { agyModels } from "@/core/agy";
import { useAgyRuntime } from "@/hooks/use-agy-runtime";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { ClaudeProviderDialog } from "@/components/settings/ClaudeProviderCard";
import { AgyProviderDialog } from "@/components/settings/AgyProviderCard";
import { ModelPicker, type ProviderModelGroup } from "./ModelPicker";

export function ExecutorModelPicker({ nativeDisabled = false, ...props }: ComponentProps<typeof ModelPicker> & { nativeDisabled?: boolean }) {
  const runtime = useClaudeRuntime();
  const agy = useAgyRuntime(false);
  const [details, setDetails] = useState(false);
  const [agyDetails, setAgyDetails] = useState(false);
  const models = claudeModels(runtime.data);
  // Keep offline agent configuration possible; execution still requires CLI login.
  const offlineModels = !runtime.loading && !runtime.data?.installed && !runtime.error && !runtime.data?.preferences?.disabledModels.includes("default")
    ? [{ value: "default", label: "Padrão do Claude Code", reasoningLevels: [], defaultReasoningLevel: null }]
    : [];
  const groups: ProviderModelGroup[] = [
    ...(nativeDisabled ? [] : props.modelGroups.filter(group => executorOf(group) === "jarvis")),
    ...(runtime.data?.preferences?.enabled === false ? [] : [{ provider: "Claude Code", executor: "claude" as const, models: runtime.data?.installed ? models : offlineModels, emptyMessage: runtime.loading ? "Consultando Claude Code…" : "Configure os modelos em Configurações → Provedores → Claude Code." }]),
    ...(agy.data?.preferences?.enabled ? [{ provider: "Antigravity CLI", executor: "agy" as const, models: agyModels(agy.data), emptyMessage: agy.loading ? "Consultando Antigravity CLI…" : "Configure os modelos em Configurações → Provedores → Antigravity CLI." }] : []),
  ];
  const claudeSelected = executorOf(props.selection) === "claude";
  const agySelected = executorOf(props.selection) === "agy";
  return <>
    <ModelPicker {...props} modelGroups={groups} onSelect={next => props.onSelect({ ...next, executor: executorOf(next) })} onRefresh={claudeSelected ? () => { void runtime.refresh(); } : agySelected ? () => { void agy.refresh(); } : props.onRefresh} refreshing={claudeSelected ? runtime.loading : agySelected ? agy.loading : props.refreshing} />
    {claudeSelected && <Hint content="Configurar instalação, conta e modelos do Claude Code"><Button type="button" size="icon-sm" variant="ghost" aria-label="Status e configuração do Claude Code" className="shrink-0 cursor-pointer" onClick={() => setDetails(true)}><CircleHelp /></Button></Hint>}
    {agySelected && <Hint content="Configurar instalação, conta e modelos do Antigravity CLI"><Button type="button" size="icon-sm" variant="ghost" aria-label="Status e configuração do Antigravity CLI" className="shrink-0 cursor-pointer" onClick={() => setAgyDetails(true)}><CircleHelp /></Button></Hint>}
    <ClaudeProviderDialog open={details} onOpenChange={setDetails} />
    <AgyProviderDialog open={agyDetails} onOpenChange={setAgyDetails} />
  </>;
}
