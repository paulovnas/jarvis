import { useId, type ComponentProps } from "react";
import { CircleHelp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export function FieldHelp({ label, children }: { label: string; children: React.ReactNode }) {
  return <Tooltip><TooltipTrigger render={<Button type="button" variant="ghost" size="icon-sm" />} aria-label={`Ajuda: ${label}`} className="size-5 shrink-0 cursor-pointer text-muted-foreground"><CircleHelp className="size-3" /></TooltipTrigger><TooltipContent className="max-w-72 text-xs leading-5">{children}</TooltipContent></Tooltip>;
}
export function Choice<T extends string>({ label, help, value, items, onChange, disabled }: { label: string; help?: string; value: T | null; items: { value: T; label: string }[]; onChange: (value: T) => void; disabled?: boolean }) {
  const id = useId();
  return <div className="min-w-0 space-y-1.5"><div className="flex items-center gap-1"><Label htmlFor={id} className="text-xs">{label}</Label>{help && <FieldHelp label={label}>{help}</FieldHelp>}</div><Select items={items} value={value} disabled={disabled} onValueChange={value => { if (value) onChange(value); }}><SelectTrigger id={id} aria-label={label} className="w-full cursor-pointer text-xs"><SelectValue placeholder="Selecionar" /></SelectTrigger><SelectContent>{items.map(item => <SelectItem key={item.value} value={item.value} className="cursor-pointer">{item.label}</SelectItem>)}</SelectContent></Select></div>;
}
export function Field({ label, help, ...props }: ComponentProps<typeof Input> & { label: string; help?: string }) {
  const id = useId();
  return <div className="min-w-0 space-y-1.5"><div className="flex items-center gap-1"><Label htmlFor={id} className="text-xs">{label}</Label>{help && <FieldHelp label={label}>{help}</FieldHelp>}</div><Input {...props} id={id} aria-label={label} className="font-mono text-xs" /></div>;
}
