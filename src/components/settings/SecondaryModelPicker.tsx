import { ExecutorModelPicker } from "@/components/chat/ExecutorModelPicker";
import type { ProviderModelGroup } from "@/components/chat/ModelPicker";
import { executionChoice, executionSelection, selectModelChoice } from "@/core/executors";
import type { ModelChoice } from "@/core/provider-references";

export function SecondaryModelPicker({ choice, modelGroups, disabled, ariaLabel, onChange }: { choice?: ModelChoice | null; modelGroups: ProviderModelGroup[]; disabled: boolean; ariaLabel: string; onChange: (choice: ModelChoice) => void }) {
  return <div className="min-w-0 space-y-1 border-t border-border pt-2">
    <p className="text-xs font-medium">Modelo secundário</p>
    <div className="flex min-w-0 flex-wrap items-center gap-1"><ExecutorModelPicker
      modelGroups={modelGroups}
      selection={executionSelection(choice?.fallback)}
      disabled={disabled || !choice}
      ariaLabel={ariaLabel}
      showProviderIdentity
      onClear={() => { if (choice) onChange({ ...choice, fallback: null }); }}
      onSelect={selection => {
        if (!choice) return;
        onChange(selectModelChoice(choice, executionChoice(selection), "secondary"));
      }}
    /></div>
    <p className="text-[10px] leading-4 text-muted-foreground">{choice ? "Usado após esgotar as tentativas do provedor principal." : "Escolha o modelo principal para configurar um secundário."}</p>
  </div>;
}
