import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { chatOptions } from "@/test/chat-fixtures";
import { RemoteQueuedMessages } from "./RemoteQueuedMessages";

const message = { id: "q1", content: "Use PostgreSQL", options: chatOptions };

it("edits in place, sends now and cancels with exact queue IDs", async () => {
  const user = userEvent.setup(); const action = vi.fn().mockResolvedValue(true);
  render(<RemoteQueuedMessages conversationId="c1" messages={[message]} running compacting={false} busy={false} onAction={action} />);
  await user.click(screen.getByRole("button", { name: "Editar mensagem 1" }));
  const input = screen.getByRole("textbox", { name: "Editar mensagem 1" });
  await user.clear(input); await user.type(input, "Use o banco local");
  await user.click(screen.getByRole("button", { name: "Salvar mensagem" }));
  await waitFor(() => expect(action).toHaveBeenCalledWith("queue_edit", { conversationId: "c1", messageId: "q1", content: "Use o banco local" }));
  await user.click(screen.getByRole("button", { name: "Enviar mensagem 1 agora" }));
  expect(action).toHaveBeenCalledWith("queue_send_now", { conversationId: "c1", messageId: "q1" });
  await user.click(screen.getByRole("button", { name: "Cancelar envio da mensagem 1" }));
  expect(action).toHaveBeenCalledWith("queue_delete", { conversationId: "c1", messageId: "q1" });
});

it("retains an unsuccessful edit and disables delivery during compaction or paused execution", async () => {
  const user = userEvent.setup(); const action = vi.fn().mockResolvedValue(false);
  const { rerender } = render(<RemoteQueuedMessages conversationId="c1" messages={[message]} running={false} compacting={false} busy={false} onAction={action} />);
  expect(screen.getByRole("button", { name: "Enviar mensagem 1 agora" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Editar mensagem 1" }));
  await user.type(screen.getByRole("textbox", { name: "Editar mensagem 1" }), " preservado");
  await user.click(screen.getByRole("button", { name: "Salvar mensagem" }));
  await waitFor(() => expect(action).toHaveBeenCalledTimes(1));
  expect(screen.getByRole("textbox", { name: "Editar mensagem 1" })).toHaveValue("Use PostgreSQL preservado");
  rerender(<RemoteQueuedMessages conversationId="c1" messages={[message]} running compacting busy={false} onAction={action} />);
  expect(screen.getByRole("button", { name: "Salvar mensagem" })).toBeDisabled();
});
