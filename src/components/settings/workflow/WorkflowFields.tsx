import { useId } from "react";
import { Label } from "@/components/ui/label";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "@/components/ui/alert-dialog";


export function ChoiceField({ label, value, options, onChange, disabled = false }: { label: string; value: string; options: { value: string; label: string }[]; onChange: (value: string) => void; disabled?: boolean }) {
  const id = useId();
  return <div className="min-w-0 space-y-1.5"><Label htmlFor={id} className="text-xs">{label}</Label><Select value={value} onValueChange={next => { if (next !== null) onChange(next); }} disabled={disabled} items={options}>
    <SelectTrigger id={id} className="w-full cursor-pointer text-xs"><SelectValue /></SelectTrigger><SelectContent>{options.map(option => <SelectItem key={option.value} value={option.value} className="cursor-pointer text-xs">{option.label}</SelectItem>)}</SelectContent>
  </Select></div>;
}


export function DiscardDialog({ open, onOpenChange, onDiscard }: { open: boolean; onOpenChange: (open: boolean) => void; onDiscard: () => void }) {
  return <AlertDialog open={open} onOpenChange={onOpenChange}><AlertDialogContent className="dark"><AlertDialogHeader><AlertDialogTitle>Descartar alterações?</AlertDialogTitle><AlertDialogDescription>As alterações deste editor ainda não foram salvas.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel className="cursor-pointer">Continuar editando</AlertDialogCancel><AlertDialogAction className="cursor-pointer" variant="destructive" onClick={onDiscard}>Descartar</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>;
}
