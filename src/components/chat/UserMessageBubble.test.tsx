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

  it("preserves ordinary messages, rich parts and attachment labels", async () => {
    render(<UserMessageBubble message={{ ...message, content: "Pedido original", parts: [{ type: "text", text: "Texto com contexto" }], attachments: [{ id: "file", name: "referencia.ts", type: "code", size: "2 KB" }] }} />);
    expect(await screen.findByText("Texto com contexto")).toBeVisible();
    expect(screen.getByText("referencia.ts")).toBeVisible();
    expect(screen.getByText("(2 KB)")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver detalhes" })).not.toBeInTheDocument();
  });

  it("keeps older Live messages without a valid payload readable", async () => {
    const legacy = "Impeccable Live · ação b0338dda\nExecute SOMENTE o pedido atual abaixo como Designer.";
    render(<UserMessageBubble message={{ ...message, content: legacy }} />);
    expect(await screen.findByText(/Execute SOMENTE/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Ver detalhes" })).not.toBeInTheDocument();
  });
});

describe("sent user Markdown", () => {
  it("renders headings, emphasis, lists and code without changing the saved source", async () => {
    const content = "OK, implemente **as correções**.\n\n### Escopo\n- **Pagamento:** confira o prazo.\n- Use `src/pdf.ts`.\n\n```ts\nconst prazo = 12;\n```";
    const sent = Object.freeze({ ...message, content });
    render(<UserMessageBubble message={sent} />);

    expect(await screen.findByRole("heading", { level: 3, name: "Escopo" })).toBeVisible();
    expect(screen.getByText("as correções").tagName).toBe("STRONG");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText("Pagamento:").tagName).toBe("STRONG");
    expect(screen.getByText("src/pdf.ts").tagName).toBe("CODE");
    expect(screen.getByRole("region", { name: "Código ts" })).toHaveTextContent("const prazo = 12;");
    expect(screen.getByText("01:12")).toBeVisible();
    expect(sent.content).toBe(content);
  });

  it("keeps inline skill chips inside the formatted text and preserves attachment labels", async () => {
    const parts: NonNullable<ChatMessage["parts"]> = [
      { type: "text", text: "## Revisão\n\n**Use " },
      { type: "skill", id: "react", name: "React Expert" },
      { type: "text", text: " agora**.\n\n- Confira `app.tsx`." },
    ];
    const sent = { ...message, content: "Texto técnico original", parts, attachments: [{ id: "file", name: "referencia.ts", type: "code" as const, size: "2 KB" }] };
    const original = JSON.stringify(sent);
    render(<UserMessageBubble message={sent} />);

    expect(await screen.findByRole("heading", { level: 2, name: "Revisão" })).toBeVisible();
    const skill = screen.getByText("React Expert");
    expect(skill).toHaveAttribute("data-slot", "badge");
    expect(skill.closest("strong")).toHaveTextContent("Use React Expert agora");
    expect(screen.getByRole("listitem")).toHaveTextContent("Confira app.tsx.");
    expect(screen.getByText("referencia.ts")).toBeVisible();
    expect(screen.queryByText("Texto técnico original")).not.toBeInTheDocument();
    expect(JSON.stringify(sent)).toBe(original);
  });

  it("keeps HTML and unsafe links inert in submitted Markdown", async () => {
    const content = "### Referência\n\n[Script](javascript:alert%281%29)\n\n<script>alert(1)</script>\n\n![Exemplo](https://example.com/private.png)";
    const { container } = render(<UserMessageBubble message={{ ...message, content }} />);
    expect(await screen.findByRole("heading", { name: "Referência" })).toBeVisible();
    expect(screen.getByText("Script")).toBeVisible();
    expect(container.querySelector("script, img, a[href^='javascript:']")).toBeNull();
    expect(screen.getByText("[Imagem: Exemplo]")).toBeVisible();
  });
});
