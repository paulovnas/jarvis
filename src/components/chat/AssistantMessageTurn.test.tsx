import { invoke } from "@tauri-apps/api/core";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssistantMessageTurn } from "./AssistantMessageTurn";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const message = {
  id: "turn-1",
  role: "assistant" as const,
  content: "# Resultado\n\nTexto com **Markdown**.",
  timestamp: "18:30",
  exportFileName: "Resposta-Jarvis-2026-09-14-18-30.md",
};

describe("ações da resposta final", () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });

  it("copies the complete original Markdown", async () => {
    const user = userEvent.setup();
    const copy = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    render(<AssistantMessageTurn message={message} />);

    await user.click(screen.getByRole("button", { name: "Copiar resposta" }));

    expect(copy).toHaveBeenCalledWith(message.content);
  });

  it("opens the native Markdown export for the complete response", async () => {
    vi.mocked(invoke).mockResolvedValue(true);
    const user = userEvent.setup();
    render(<AssistantMessageTurn message={message} />);

    await user.click(screen.getByRole("button", { name: "Salvar resposta em Markdown" }));

    expect(invoke).toHaveBeenCalledWith("save_markdown_document", {
      content: message.content,
      suggestedFileName: message.exportFileName,
    });
  });

  it("keeps export actions hidden until the response is final", () => {
    render(<AssistantMessageTurn message={{ ...message, streaming: true }} />);

    expect(screen.queryByRole("group", { name: "Ações da resposta" })).not.toBeInTheDocument();
  });
});
