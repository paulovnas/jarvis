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
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";

interface ItemNameDialogProps {
  kind: "workspace" | "project" | "conversation";
  initialValue?: string;
  projectPath?: string;
  pending: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (name: string) => Promise<boolean>;
}

export function ItemNameDialog({
  kind,
  initialValue = "",
  projectPath,
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
                : kind === "project"
                  ? "Editar projeto"
                  : "Editar conversa"}
            </DialogTitle>
            <DialogDescription>
              {isWorkspace
                ? "Agrupe seus projetos. O workspace não possui pasta nem configurações próprias."
                : kind === "project"
                  ? "Altere o nome exibido para este projeto. A pasta permanece a mesma."
                  : "Escolha um título curto para identificar a conversa."}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field data-invalid={!!message} data-disabled={pending}>
              <FieldLabel htmlFor={id}>
                {isWorkspace
                  ? "Nome do workspace"
                  : kind === "project"
                    ? "Nome do projeto"
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
            {kind === "project" && (
              <Field>
                <FieldLabel htmlFor={`${id}-path`}>Local do projeto</FieldLabel>
                <Input id={`${id}-path`} value={projectPath ?? ""} readOnly />
              </Field>
            )}
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
