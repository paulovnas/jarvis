import { useId, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/TextInput";
import { Spinner } from "@/components/ui/spinner";

interface ItemNameDialogProps {
  kind: "workspace" | "conversation";
  initialValue?: string;
  pending: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (name: string) => Promise<boolean>;
}

export function ItemNameDialog({
  kind,
  initialValue = "",
  pending,
  error,
  onClose,
  onSubmit,
}: ItemNameDialogProps) {
  const id = useId();
  const [value, setValue] = useState(initialValue);
  const [validation, setValidation] = useState<string | null>(null);
  const isWorkspace = kind === "workspace";
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (pending) return;
    const name = value.trim();
    if (
      !name ||
      [...name].length > 120 ||
      [...name].some(
        (char) =>
          char.charCodeAt(0) < 32 ||
          (char.charCodeAt(0) >= 127 && char.charCodeAt(0) <= 159),
      )
    ) {
      setValidation(
        "Informe um nome de até 120 caracteres, sem quebras de linha.",
      );
      return;
    }
    setValidation(null);
    if (await onSubmit(name)) onClose();
  };
  const message = validation ?? error;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !pending) onClose();
      }}
    >
      <DialogContent showCloseButton={false}>
        <form
          onSubmit={(event) => {
            void submit(event);
          }}
          className="flex flex-col gap-4"
        >
          <DialogHeader>
            <DialogTitle>
              {isWorkspace
                ? "Novo workspace"
                : "Editar conversa"}
            </DialogTitle>
            <DialogDescription>
              {isWorkspace
                ? "Agrupe seus projetos. O workspace não possui pasta nem configurações próprias."
                : "Escolha um título curto para identificar a conversa."}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field data-invalid={!!message} data-disabled={pending}>
              <FieldLabel htmlFor={id}>
                {isWorkspace
                  ? "Nome do workspace"
                  : "Título da conversa"}
              </FieldLabel>
              <Input
                id={id}
                autoFocus
                value={value}
                disabled={pending}
                onChange={(event) => {
                  setValue(event.target.value);
                  setValidation(null);
                }}
                aria-invalid={!!message}
                aria-describedby={message ? `${id}-error` : undefined}
              />
              {message && (
                <FieldError id={`${id}-error`} role="alert">
                  {message}
                </FieldError>
              )}
            </Field>
          </FieldGroup>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              className="cursor-pointer"
              disabled={pending}
              onClick={onClose}
            >
              Cancelar
            </Button>
            <Button
              type="submit"
              className="cursor-pointer"
              disabled={pending || !value.trim()}
            >
              {pending && <Spinner />}
              {isWorkspace ? "Criar workspace" : "Salvar"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
