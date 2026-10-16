import { useId } from "react";
import { Input } from "@/components/TextInput";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { validateProviderDisplayName } from "@/core/provider-accounts";

export function ProviderDisplayNameField({ value, onChange, disabled = false, required = true }: { value: string; onChange: (value: string) => void; disabled?: boolean; required?: boolean }) {
  const id = useId();
  return <Field>
    <FieldLabel htmlFor={id}>Nome de exibição</FieldLabel>
    <Input id={id} value={value} onChange={event => onChange(event.currentTarget.value)} disabled={disabled} aria-required={required} aria-invalid={value !== "" && validateProviderDisplayName(value) !== null} aria-describedby={`${id}-help`} autoComplete="off" />
    <FieldDescription id={`${id}-help`}>Texto livre, de 1 a 18 caracteres. O alias interno permanece o mesmo.</FieldDescription>
  </Field>;
}
