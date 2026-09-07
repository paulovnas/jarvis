import { useRef } from "react";
import { ConfirmationDialogContent as AlertDialogContent } from "@/components/ConfirmationDialogContent";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel,
  AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Spinner } from "@/components/ui/spinner";

export function DeleteItemDialog({ kind, name, conversationCount, pending, error, onClose, onConfirm }: {
  kind: "project" | "conversation";
  name: string;
  conversationCount: number;
  pending: boolean;
  error: string | null;
  onClose: () => void;
  onConfirm: () => Promise<boolean>;
}) {
  const submitting = useRef(false);
  const confirm = async () => {
    if (pending || submitting.current) return;
    submitting.current = true;
    try { if (await onConfirm()) onClose(); }
    finally { submitting.current = false; }
  };
  return <AlertDialog open onOpenChange={open => { if (!open && !pending && !submitting.current) onClose(); }}>
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{kind === "project" ? "Excluir projeto?" : "Excluir conversa?"}</AlertDialogTitle>
        <AlertDialogDescription>
          {kind === "project"
            ? <>O projeto <strong className="break-words">{name}</strong> será removido do Jarvis junto com todas as suas conversas ({conversationCount}) e seus históricos. A pasta do projeto e seus arquivos permanecerão intactos.</>
            : <>A conversa <strong className="break-words">{name}</strong> e todo o seu histórico serão apagados. Os arquivos do projeto permanecerão intactos.</>}
          <span className="mt-2 block">Esta exclusão é definitiva e não pode ser desfeita.</span>
        </AlertDialogDescription>
      </AlertDialogHeader>
      {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
      <AlertDialogFooter>
        <AlertDialogCancel disabled={pending} className="cursor-pointer">Cancelar</AlertDialogCancel>
        <AlertDialogAction variant="destructive" disabled={pending} className="cursor-pointer" onClick={() => { void confirm(); }}>
          {pending && <Spinner aria-label="Excluindo" />}
          {pending ? "Excluindo…" : kind === "project" ? "Excluir projeto" : "Excluir conversa"}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>;
}
