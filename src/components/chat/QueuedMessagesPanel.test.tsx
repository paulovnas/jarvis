import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { QueuedMessage } from "@/core/chat";
import { chatOptions } from "@/test/chat-fixtures";
import { QueuedMessagesPanel } from "./QueuedMessagesPanel";

const messages: QueuedMessage[] = [
  { id: "first", content: "Primeira orientação", options: chatOptions },
  { id: "second", content: "Segunda orientação", options: chatOptions },
];

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const item = this.closest("[data-queued-message]");
    const index = item ? [...document.querySelectorAll("[data-queued-message]")].indexOf(item) : 0;
    return { x: 0, y: index * 48, top: index * 48, left: 0, right: 600, bottom: index * 48 + 40, width: 600, height: 40, toJSON: () => ({}) };
  });
});

afterEach(() => vi.restoreAllMocks());

describe("QueuedMessagesPanel", () => {
  it("reorders messages and exposes immediate delivery and editing while the agent runs", async () => {
    const user = userEvent.setup();
    const reorder = vi.fn().mockResolvedValue(true);
    const sendNow = vi.fn().mockResolvedValue(true);
    const edit = vi.fn().mockResolvedValue(undefined);
    render(<QueuedMessagesPanel messages={messages} running compacting={false} onReorder={reorder} onSendNow={sendNow} onEdit={edit} />);

    expect(screen.getAllByRole("button", { name: /Reordenar mensagem/ })).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Enviar mensagem 1 agora" }));
    expect(sendNow).toHaveBeenCalledWith("first");
    await user.click(screen.getByRole("button", { name: "Editar mensagem 2" }));
    expect(edit).toHaveBeenCalledWith("second");
    screen.getByRole("button", { name: "Reordenar mensagem 1" }).focus();
    await user.keyboard(" ");
    await user.keyboard("{ArrowDown}");
    await user.keyboard(" ");
    await waitFor(() => expect(reorder).toHaveBeenCalledWith(["second", "first"]));
  });

  it("confirms permanent deletion and keeps immediate delivery disabled in a paused queue", async () => {
    const user = userEvent.setup();
    const remove = vi.fn().mockResolvedValue(true);
    const resume = vi.fn().mockResolvedValue(undefined);
    render(<QueuedMessagesPanel messages={[messages[0]]} running={false} compacting={false} onDelete={remove} onResume={resume} onSendNow={vi.fn()} />);

    expect(screen.queryByRole("button", { name: /Reordenar mensagem/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enviar mensagem 1 agora" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Excluir mensagem 1" }));
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("não voltará para o campo de texto");
    await user.click(screen.getByRole("button", { name: "Excluir mensagem" }));
    await waitFor(() => expect(remove).toHaveBeenCalledWith("first"));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Continuar fila" }));
    expect(resume).toHaveBeenCalledTimes(1);
  });
});
