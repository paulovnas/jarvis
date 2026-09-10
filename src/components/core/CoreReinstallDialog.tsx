import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";

export function CoreReinstallDialog({
  name,
  open,
  busy,
  onOpenChange,
  onConfirm,
}: {
  name?: string;
  open: boolean;
  busy: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  return <AlertDialog open={open} onOpenChange={next => { if (!busy) onOpenChange(next); }}>
    <AlertDialogContent className="dark">
      <AlertDialogHeader>
        <AlertDialogTitle>Reinstalar {name ?? "componente"}?</AlertDialogTitle>
        <AlertDialogDescription>Uma instalação nova será baixada e verificada antes de substituir o pacote atual. Projetos, conversas e chaves serão preservados.</AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <Button variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>Cancelar</Button>
        <Button data-confirm-action variant="destructive" disabled={busy} onClick={onConfirm}>Confirmar reinstalação</Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>;
}
