import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { AssistantMessageTurn } from "./AssistantMessageTurn";
import type { ChatMessage, ToolCallItem } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const image = { id: "a1", conversationId: "chat1", name: "imagem-1.png", mime: "image/png", kind: "image", size: 1200 };
const result = JSON.stringify({ kind: "generated_image", model: "gemini-3.1-flash-image", accountAlias: "google", images: [image], text: "" });
const message = (tool: ToolCallItem): ChatMessage => ({ id: "m1", role: "assistant", timestamp: "12:00", content: "", streaming: tool.status === "running", work: { durationSeconds: 1, steps: [{ thinking: "", commentary: "", tools: [tool] }] } });

it("shows a skeleton without opening activity, then the persisted image and export dialog", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockImplementation(async command => command === "save_chat_image" ? true : "data:image/png;base64,aW1hZ2U=");
  const tool: ToolCallItem = { id: "t1", name: "generate_image", status: "running" };
  const { rerender } = render(<AssistantMessageTurn message={message(tool)} />);
  expect(screen.getByRole("status", { name: "Gerando imagem" })).toBeVisible();
  rerender(<AssistantMessageTurn message={message({ ...tool, status: "completed", output: result })} />);
  expect(screen.queryByRole("status", { name: "Gerando imagem" })).not.toBeInTheDocument();
  expect(screen.getByRole("status", { name: "Carregando imagem" })).toBeVisible();
  const img = await screen.findByRole("img", { name: image.name });
  fireEvent.load(img);
  await user.click(screen.getByRole("button", { name: `Ampliar ${image.name}` }));
  expect(await screen.findByRole("dialog")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Salvar imagem" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_chat_image", { conversationId: "chat1", id: "a1" }));
});
it("replaces the skeleton with the failed generation message", () => {
  render(<AssistantMessageTurn message={message({ id: "t1", name: "generate_image", status: "error", output: "O limite de geração de imagens foi atingido." })} />);
  expect(screen.getByRole("alert")).toHaveTextContent("O limite de geração");
  expect(screen.queryByRole("status", { name: "Gerando imagem" })).not.toBeInTheDocument();
});
