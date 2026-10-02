import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CompanionItem } from "@/core/companion";
import { CompanionNotifications } from "./CompanionNotifications";

const item = (index: number): CompanionItem => ({
  conversationId: `chat-${index}`, agentId: null, projectId: "project", projectName: "Portal", global: false,
  title: `Atividade ${index}`, role: "builder", status: "completed", activity: "Concluído", result: `Resultado ${index}`,
  tasks: [],
  durationMs: 1000, activeSince: null, updatedAt: index, requiresConversation: false, attentionId: `chat-${index}/completed`, acknowledged: false,
});

describe("Jarvito notification cascade", () => {
  it("uses Sonner's stack and only exposes the front result until OK dismisses it", async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn(async () => true);
    const onOpen = vi.fn(async () => {});
    const items = [item(1), item(2), item(3)];
    const view = render(<CompanionNotifications items={items} error={null} onDismiss={onDismiss} onOpen={onOpen} />);
    expect(await screen.findByRole("heading", { name: "Atividade 3" })).toBeVisible();
    const toaster = view.container.querySelector("[data-sonner-toaster]");
    expect(toaster?.querySelectorAll("[data-sonner-toast]")).toHaveLength(3);
    const front = toaster?.querySelector("[data-front=true]");
    expect(front).toHaveTextContent("Atividade 3");
    expect(screen.queryByRole("heading", { name: "Atividade 2" })).not.toBeInTheDocument();
    for (const behind of toaster?.querySelectorAll("[data-front=false] button") ?? []) expect(behind).toBeDisabled();
    fireEvent.click(screen.getByRole("heading", { name: "Atividade 3" }));
    expect(onOpen).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onDismiss).toHaveBeenCalledExactlyOnceWith(items[2]);
    expect(onOpen).not.toHaveBeenCalled();
    view.rerender(<CompanionNotifications items={items.slice(0, 2)} error={null} onDismiss={onDismiss} onOpen={onOpen} />);
    await waitFor(() => expect(toaster?.querySelector("[data-front=true]:not([data-removed=true])")).toHaveTextContent("Atividade 2"));
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Atividade 3" })).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Ver atividade" }));
    expect(onOpen).toHaveBeenCalledExactlyOnceWith(items[1]);
  });

  it("keeps a newly arriving completion enabled at the front of an existing stack", async () => {
    const onDismiss = vi.fn(async () => true);
    const onOpen = vi.fn(async () => {});
    const view = render(<CompanionNotifications items={[item(1), item(2)]} error={null} onDismiss={onDismiss} onOpen={onOpen} />);
    await screen.findByRole("heading", { name: "Atividade 2" });
    const latest = item(3);
    view.rerender(<CompanionNotifications items={[latest, item(1), item(2)]} error={null} onDismiss={onDismiss} onOpen={onOpen} />);
    await waitFor(() => expect(view.container.querySelector("[data-front=true]")).toHaveTextContent("Atividade 3"));
    expect(screen.getByRole("button", { name: "OK" })).toBeEnabled();
    expect(screen.queryByRole("heading", { name: "Atividade 2" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Ver atividade" }));
    expect(onOpen).toHaveBeenCalledExactlyOnceWith(latest);
  });

  it("communicates a failed general request and leaves its confirmation available after an error", async () => {
    const onDismiss = vi.fn(async () => false);
    const onOpen = vi.fn(async () => {});
    const failed = { ...item(1), global: true, status: "failed" as const, activity: "O provedor ficou indisponível." };
    const view = render(<CompanionNotifications items={[failed]} error={null} onDismiss={onDismiss} onOpen={onOpen} />);
    const notice = await screen.findByLabelText("Notificação: Atividade 1");
    expect(within(notice).getByRole("heading")).toHaveTextContent("Não consegui concluir sua solicitação.");
    expect(notice).not.toHaveTextContent("Sua resposta está pronta.");
    fireEvent.click(within(notice).getByRole("button", { name: "OK" }));
    view.rerender(<CompanionNotifications items={[failed]} error="Não foi possível confirmar a atividade." onDismiss={onDismiss} onOpen={onOpen} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível confirmar a atividade.");
    expect(screen.getByRole("button", { name: "OK" })).toBeEnabled();
    expect(onOpen).not.toHaveBeenCalled();
  });
});
