import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, within } from "@testing-library/react";
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

describe("assistant metadata header", () => {
  const work = { durationSeconds: 12, steps: [{ thinking: "Verificando o contexto", commentary: "", tools: [{ id: "read", name: "read_file", status: "completed" as const, output: "README" }] }] };
  it("puts the model before the completed-work disclosure on the same header without a brand assembly", async () => {
    const user = userEvent.setup();
    const view = render(<AssistantMessageTurn message={{ ...message, model: "provider / muse-spark-1.3-contributor", work }} />);
    const header = view.container.querySelector("header");
    expect(header).not.toBeNull();
    const model = within(header!).getByText("muse-spark-1.3-contributor");
    const disclosure = within(header!).getByRole("button", { name: /Trabalhou por 12s/ });
    expect(model.compareDocumentPosition(disclosure) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(header!).getByText("18:30")).toBeVisible();
    expect(screen.queryByRole("img", { name: "Jarvis" })).not.toBeInTheDocument();
    expect(screen.queryByText("arvis")).not.toBeInTheDocument();
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    const divider = screen.getByRole("separator");
    expect(disclosure.compareDocumentPosition(divider) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(divider.compareDocumentPosition(view.container.querySelector(".assistant-prose")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.click(disclosure);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("list", { name: "Etapas da execução" })).toBeVisible();
  });

  it.each(["thinking", "tool"])("keeps only the model and timestamp above %s activity", state => {
    const activeWork = { ...work, steps: [{ ...work.steps[0], tools: state === "thinking" ? [] : [{ ...work.steps[0].tools[0], status: "running" as const }] }] };
    const view = render(<AssistantMessageTurn message={{ ...message, content: "", model: "provider / GPT-6", work: activeWork, streaming: true }} />);
    const header = view.container.querySelector("header");
    expect(header).not.toBeNull();
    expect(within(header!).getByText("GPT-6")).toBeVisible();
    expect(within(header!).getByText("18:30")).toBeVisible();
    expect(within(header!).queryByRole("button")).not.toBeInTheDocument();
    expect(screen.queryByText(/Trabalhou por/)).not.toBeInTheDocument();
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "Jarvis" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Atividades da execução atual" })).toBeVisible();
  });
});

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

it("buffers live answer updates and immediately flushes an error or a new message identity", async () => {
  await import("./ChatMarkdown");
  vi.useFakeTimers();
  const frames = new Map<number, FrameRequestCallback>();
  let id = 0;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.set(++id, callback); return id; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(frame => { frames.delete(frame); });
  const view = render(<AssistantMessageTurn message={{ ...message, content: "Texto **base**", streaming: true }} />);
  try {
    await act(async () => {});
    expect(view.container.querySelector("strong")).toHaveTextContent("base");
    view.rerender(<AssistantMessageTurn message={{ ...message, content: "Texto **base** recebendo mais conteúdo", streaming: true }} />);
    expect(view.container.querySelector(".assistant-prose")).toHaveTextContent(/^Texto base$/);
    expect(frames.size).toBe(1);

    view.rerender(<AssistantMessageTurn message={{ ...message, content: "Texto **base** recebendo mais conteúdo", streaming: true, error: { title: "Interrompido", message: "Falha" } }} />);
    expect(view.container.querySelector(".assistant-prose")).toHaveTextContent("Texto base recebendo mais conteúdo");
    expect(frames.size).toBe(0);
    view.rerender(<AssistantMessageTurn message={{ ...message, id: "turn-2", content: "Texto **base** de outra execução", streaming: true }} />);
    expect(view.container.querySelector(".assistant-prose")).toHaveTextContent("Texto base de outra execução");
  } finally {
    view.unmount();
    vi.useRealTimers();
    vi.restoreAllMocks();
  }
});
