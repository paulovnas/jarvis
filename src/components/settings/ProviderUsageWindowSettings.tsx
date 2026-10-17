import { useId } from "react";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { availableUsageAlertWindows, type UsageAlertWindow } from "@/core/provider-usage";
import { useProviderUsage } from "@/hooks/use-provider-usage";

type WindowSettingsProps = {
  providerName: string;
  accountAlias?: string;
  showFiveHourUsage?: boolean;
  showWeeklyUsage?: boolean;
  disabled: boolean;
  onChange: (showFiveHourUsage: boolean, showWeeklyUsage: boolean) => void;
};

export function ProviderUsageWindowSettings(props: WindowSettingsProps) {
  return props.accountAlias ? <ReportedWindows {...props} accountAlias={props.accountAlias} /> : <WindowChoices {...props} />;
}

function ReportedWindows(props: WindowSettingsProps & { accountAlias: string }) {
  const { data } = useProviderUsage(props.accountAlias);
  const available = availableUsageAlertWindows(data?.windows ?? [], false);
  return available.length ? <WindowChoices {...props} available={available} /> : null;
}

function WindowChoices({ providerName, showFiveHourUsage, showWeeklyUsage, disabled, onChange, available = ["five_hour", "weekly"] }: WindowSettingsProps & { available?: UsageAlertWindow[] }) {
  const id = useId();
  return <FieldSet disabled={disabled} className="gap-2">
    <FieldLegend variant="label" className="sr-only">Janelas de {providerName} na barra de status</FieldLegend>
    <FieldGroup className="flex-row flex-wrap gap-4">
      {available.includes("five_hour") && <Field orientation="horizontal" data-disabled={disabled} className="w-auto">
        <Checkbox id={`${id}-five-hour`} checked={showFiveHourUsage !== false} disabled={disabled} className="cursor-pointer" onCheckedChange={checked => onChange(checked, showWeeklyUsage !== false)} />
        <FieldLabel htmlFor={`${id}-five-hour`} className="cursor-pointer text-xs">5 horas</FieldLabel>
      </Field>}
      {available.includes("weekly") && <Field orientation="horizontal" data-disabled={disabled} className="w-auto">
        <Checkbox id={`${id}-weekly`} checked={showWeeklyUsage !== false} disabled={disabled} className="cursor-pointer" onCheckedChange={checked => onChange(showFiveHourUsage !== false, checked)} />
        <FieldLabel htmlFor={`${id}-weekly`} className="cursor-pointer text-xs">Semanal</FieldLabel>
      </Field>}
    </FieldGroup>
  </FieldSet>;
}
