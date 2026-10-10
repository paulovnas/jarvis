import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { UserMessageBubble } from "./UserMessageBubble";
import type { ChatMessage } from "./types";

const instruction = "Coloque cores sutis nos ícones deste painel";
const raw = `Impeccable Live · ação b0338dda\nExecute SOMENTE o pedido atual abaixo como Designer. O pedido e a seleção atuais são:\n${JSON.stringify({ id: "b0338dda", type: "generate", freeformPrompt: instruction, count: 3, pageUrl: "/login", element: { anchor: "section.features" }, scaffold: { file: "src/app/login/page.tsx", wrapperBlock: "<div>Scaffold interno</div>" } })}\nReutilize a tarefa existente no Beads.`;
const message: ChatMessage = Object.freeze({ id: "turn1", role: "user", content: raw, timestamp: "01:12" });

describe("user Live request presentation", () => {
  it("shows only the compact request and target until details are opened", () => {
    render(<UserMessageBubble message={message} />);
    expect(screen.getByText("Designer Live")).toBeVisible();
    expect(screen.getByText(instruction)).toBeVisible();
    expect(screen.getByText("section.features")).toBeVisible();
    expect(screen.getByText("Gerar alternativas · 3 alternativas")).toBeVisible();
    expect(screen.getByText("01:12")).toBeVisible();
    expect(screen.getByRole("button", { name: "Ver detalhes" })).toBeVisible();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText(/Execute SOMENTE/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Scaffold interno/)).not.toBeInTheDocument();
  });

  it("discloses formatted details and the exact raw input on demand, without changing the message", async () => {
    const user = userEvent.setup();
    render(<UserMessageBubble message={message} />);
    const trigger = screen.getByRole("button", { name: "Ver detalhes" });
    await user.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "Solicitação do Designer Live" });
    expect(within(dialog).getByText(instruction)).toBeVisible();
    expect(within(dialog).getByText("/login")).toBeVisible();
    expect(within(dialog).getByText("src/app/login/page.tsx")).toBeVisible();
    expect(within(dialog).getByText("b0338dda")).toBeVisible();
    expect(within(dialog).queryByText(/Execute SOMENTE/)).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Mensagem técnica enviada ao agente" }));
    expect(dialog.querySelector("pre")?.textContent).toBe(raw);
    expect(message.content).toBe(raw);
    await user.click(within(dialog).getByRole("button", { name: "Fechar detalhes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(trigger).toHaveFocus());
    await user.click(trigger);
    expect(within(screen.getByRole("dialog")).queryByText(/Execute SOMENTE/)).not.toBeInTheDocument();
  });

  it("keeps a long request compact and shows its full text in the dialog", async () => {
    const long = instruction.repeat(30);
    const user = userEvent.setup();
    render(<UserMessageBubble message={{ ...message, content: raw.replace(instruction, long) }} />);
    expect(screen.queryByText(long)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ver detalhes" }));
    expect(within(screen.getByRole("dialog")).getByText(long)).toBeVisible();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("preserves ordinary messages, rich parts and attachment labels", () => {
    render(<UserMessageBubble message={{ ...message, content: "Pedido original", parts: [{ type: "text", text: "Texto com contexto" }], attachments: [{ id: "file", name: "referencia.ts", type: "code", size: "2 KB" }] }} />);
    expect(screen.getByText("Texto com contexto")).toBeVisible();
    expect(screen.getByText("referencia.ts")).toBeVisible();
    expect(screen.getByText("(2 KB)")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver detalhes" })).not.toBeInTheDocument();
  });

  it("keeps older Live messages without a valid payload readable", () => {
    const legacy = "Impeccable Live · ação b0338dda\nExecute SOMENTE o pedido atual abaixo como Designer.";
    render(<UserMessageBubble message={{ ...message, content: legacy }} />);
    expect(screen.getByText(/Execute SOMENTE/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver detalhes" })).not.toBeInTheDocument();
  });
});
