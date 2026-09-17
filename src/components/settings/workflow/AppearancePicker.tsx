import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { IdentityAppearance } from "@/core/workflow-appearance";
import { WORKFLOW_COLORS, WORKFLOW_ICONS, WORKFLOW_ICON_KEYS } from "@/components/agents/workflow-appearance";
import { WorkflowIdentityIcon } from "@/components/agents/WorkflowIdentityIcon";
import { Hint } from "@/components/ui/hint";

export function AppearancePicker<T extends IdentityAppearance>({ value, onChange, disabled, icons = WORKFLOW_ICON_KEYS }: { value: T; onChange: (value: T) => void; disabled: boolean; icons?: readonly IdentityAppearance["icon"][] }) {
  return <div className="space-y-3 rounded-md border border-border bg-sidebar/50 p-3">
    <div className="flex items-center gap-2 text-xs font-medium"><WorkflowIdentityIcon appearance={value} className="size-4" />Identidade visual</div>
    <fieldset disabled={disabled} className="min-w-0 space-y-2"><legend className="text-xs text-muted-foreground">Cor</legend>
      <div className="flex flex-wrap gap-0.5">{(Object.keys(WORKFLOW_COLORS) as IdentityAppearance["color"][]).map(color => <Hint key={color} content={WORKFLOW_COLORS[color].label}><Button type="button" variant="ghost" size="icon" aria-label={`Cor ${WORKFLOW_COLORS[color].label}`} aria-pressed={value.color === color} className="size-7 shrink-0 cursor-pointer rounded-md p-1 aria-pressed:bg-secondary aria-pressed:ring-1 aria-pressed:ring-ring" onClick={() => onChange({ ...value, color } as T)}>
        <span className="flex size-4 items-center justify-center rounded-full" style={{ backgroundColor: WORKFLOW_COLORS[color].value }}>{value.color === color && <Check className="size-3 text-sidebar" />}</span>
      </Button></Hint>)}</div>
    </fieldset>
    <fieldset disabled={disabled} className="min-w-0 space-y-2"><legend className="text-xs text-muted-foreground">Ícone <span className="ml-1 text-foreground">{WORKFLOW_ICONS[value.icon].label}</span></legend>
      <div className="flex max-w-80 flex-wrap gap-1">{icons.map(icon => {
        const { Icon, label } = WORKFLOW_ICONS[icon];
        return <Hint key={icon} content={label}><Button type="button" variant="ghost" size="icon" aria-label={`Ícone ${label}`} aria-pressed={value.icon === icon} className="size-8 shrink-0 cursor-pointer p-1 aria-pressed:bg-secondary aria-pressed:ring-1 aria-pressed:ring-ring" style={{ color: WORKFLOW_COLORS[value.color].value }} onClick={() => onChange({ ...value, icon } as T)}><Icon className="size-4" /></Button></Hint>;
      })}</div>
    </fieldset>
  </div>;
}
