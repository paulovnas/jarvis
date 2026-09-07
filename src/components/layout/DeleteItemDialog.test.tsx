import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { DeleteItemDialog } from "./DeleteItemDialog";

it("confirms deletion with Enter and prevents duplicate submission while awaiting the result", async () => {
  const user = userEvent.setup();
  const close = vi.fn();
  let resolve!: (value: boolean) => void;
  const confirm = vi.fn(() => new Promise<boolean>(done => { resolve = done; }));
  render(<DeleteItemDialog kind="conversation" name="Teste" conversationCount={1} pending={false} error={null} onClose={close} onConfirm={confirm} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Excluir conversa" })).toHaveFocus());
  await user.keyboard("{Enter}{Enter}");
  expect(confirm).toHaveBeenCalledTimes(1);
  expect(close).not.toHaveBeenCalled();
  resolve(true);
  await waitFor(() => expect(close).toHaveBeenCalledTimes(1));
});
