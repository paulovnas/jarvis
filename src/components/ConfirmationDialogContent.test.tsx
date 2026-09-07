import { useState } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogFooter, AlertDialogTitle, AlertDialogTrigger } from "@/components/ui/alert-dialog";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { ConfirmationDialogContent } from "./ConfirmationDialogContent";

function Confirmation({ onConfirm, disabled = false }: { onConfirm: () => void; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  return <AlertDialog open={open} onOpenChange={setOpen}>
    <AlertDialogTrigger render={<Button />}>Remover</AlertDialogTrigger>
    <ConfirmationDialogContent aria-describedby={undefined}>
      <AlertDialogTitle>Excluir item?</AlertDialogTitle>
      <AlertDialogFooter>
        <AlertDialogCancel>Cancelar</AlertDialogCancel>
        <AlertDialogAction disabled={disabled} onClick={() => { onConfirm(); setOpen(false); }}>Excluir</AlertDialogAction>
      </AlertDialogFooter>
    </ConfirmationDialogContent>
  </AlertDialog>;
}

it("focuses the confirmation action and submits with Enter inside a nested modal", async () => {
  const user = userEvent.setup();
  const confirm = vi.fn();
  render(<Dialog><DialogTrigger render={<Button />}>Configurações</DialogTrigger><DialogContent aria-describedby={undefined}><DialogTitle>Conta</DialogTitle><Confirmation onConfirm={confirm} /></DialogContent></Dialog>);
  await user.click(screen.getByRole("button", { name: "Configurações" }));
  await user.click(screen.getByRole("button", { name: "Remover" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Excluir" })).toHaveFocus());
  await user.keyboard("{Enter}");
  expect(confirm).toHaveBeenCalledTimes(1);
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  expect(screen.getByRole("dialog", { name: "Conta" })).toBeVisible();
});

it("lets Tab choose Cancel and Enter dismiss without confirming", async () => {
  const user = userEvent.setup(); const confirm = vi.fn();
  render(<Confirmation onConfirm={confirm} />);
  await user.click(screen.getByRole("button", { name: "Remover" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Excluir" })).toHaveFocus());
  await user.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Cancelar" })).toHaveFocus();
  await user.keyboard("{Enter}");
  expect(confirm).not.toHaveBeenCalled();
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
});

it("does not submit a disabled action and keeps Escape available", async () => {
  const user = userEvent.setup(); const confirm = vi.fn();
  render(<Confirmation onConfirm={confirm} disabled />);
  await user.click(screen.getByRole("button", { name: "Remover" }));
  const dialog = screen.getByRole("alertdialog");
  expect(within(dialog).getByRole("button", { name: "Excluir" })).toBeDisabled();
  await waitFor(() => expect(dialog).toHaveFocus());
  await user.keyboard("{Enter}");
  expect(confirm).not.toHaveBeenCalled();
  expect(dialog).toBeVisible();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
});
