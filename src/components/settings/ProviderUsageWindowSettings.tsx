import { useId } from "react";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";

export function ProviderUsageWindowSettings({ providerName, showFiveHourUsage, showWeeklyUsage, disabled, onChange }: {
  providerName: string;
  showFiveHourUsage?: boolean;
  showWeeklyUsage?: boolean;
  disabled: boolean;
  onChange: (showFiveHourUsage: boolean, showWeeklyUsage: boolean) => void;
}) {
  const id = useId();
  return <FieldSet disabled={disabled} className="gap-2">
    <FieldLegend variant="label" className="sr-only">Janelas de {providerName} na barra de status</FieldLegend>
    <FieldGroup className="flex-row flex-wrap gap-4">
      <Field orientation="horizontal" data-disabled={disabled} className="w-auto">
        <Checkbox id={`${id}-five-hour`} checked={showFiveHourUsage !== false} disabled={disabled} className="cursor-pointer" onCheckedChange={checked => onChange(checked, showWeeklyUsage !== false)} />
        <FieldLabel htmlFor={`${id}-five-hour`} className="cursor-pointer text-xs">5 horas</FieldLabel>
      </Field>
      <Field orientation="horizontal" data-disabled={disabled} className="w-auto">
        <Checkbox id={`${id}-weekly`} checked={showWeeklyUsage !== false} disabled={disabled} className="cursor-pointer" onCheckedChange={checked => onChange(showFiveHourUsage !== false, checked)} />
        <FieldLabel htmlFor={`${id}-weekly`} className="cursor-pointer text-xs">Semanal</FieldLabel>
      </Field>
    </FieldGroup>
  </FieldSet>;
}
