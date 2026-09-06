import { useState } from "react";
import { readFileSync } from "node:fs";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeAll, expect, it } from "vitest";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { AlertDialog, AlertDialogCancel, AlertDialogContent, AlertDialogTitle, AlertDialogTrigger } from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/TextInput";

// Vitest stubs CSS imports; load the real design layer to exercise its computed styles.
const applicationStyles = readFileSync("src/index.css", "utf8");

const style = document.createElement("style");
beforeAll(() => { style.textContent = applicationStyles; document.head.append(style); });
afterAll(() => { style.remove(); });

function LayeredDialogs() {
  const [open, setOpen] = useState(false);
  return <Dialog open><DialogContent aria-describedby={undefined}><DialogTitle>Configurações</DialogTitle>
    <Input aria-label="Busca" defaultValue="openrouter" />
    <Dialog open={open} onOpenChange={setOpen}><DialogTrigger render={<Button />}>Abrir provedor</DialogTrigger>
      <DialogContent aria-describedby={undefined}><DialogTitle>Provedor</DialogTitle>
        <AlertDialog><AlertDialogTrigger render={<Button />}>Remover</AlertDialogTrigger>
          <AlertDialogContent aria-describedby={undefined}><AlertDialogTitle>Confirmar</AlertDialogTitle><AlertDialogCancel>Cancelar</AlertDialogCancel></AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  </DialogContent></Dialog>;
}

it("blurs only covered dialogs and restores each layer as the top one closes", async () => {
  const user = userEvent.setup(); render(<LayeredDialogs />);
  const settings = screen.getByRole("dialog", { name: "Configurações" });
  const trigger = screen.getByRole("button", { name: "Abrir provedor" });
  expect(getComputedStyle(settings).filter).not.toContain("blur");
  await user.click(trigger);
  const provider = await screen.findByRole("dialog", { name: "Provedor" });
  await waitFor(() => expect(getComputedStyle(settings).filter).toBe("blur(3px) brightness(0.65)"), { timeout: 1000 });
  expect(getComputedStyle(provider).filter).not.toContain("blur");
  await user.click(within(provider).getByRole("button", { name: "Remover" }));
  const confirmation = await screen.findByRole("alertdialog", { name: "Confirmar" });
  await waitFor(() => expect(getComputedStyle(provider).filter).toContain("blur(3px)"), { timeout: 1000 });
  expect(getComputedStyle(confirmation).filter).not.toContain("blur");
  await user.click(within(confirmation).getByRole("button", { name: "Cancelar" }));
  await waitFor(() => expect(getComputedStyle(provider).filter).not.toContain("blur"), { timeout: 1000 });
  expect(getComputedStyle(settings).filter).toContain("blur(3px)");
  await user.keyboard("{Escape}");
  await waitFor(() => expect(getComputedStyle(settings).filter).not.toContain("blur"), { timeout: 1000 });
  expect(screen.getByLabelText("Busca")).toHaveValue("openrouter");
  await waitFor(() => expect(trigger).toHaveFocus());
});
