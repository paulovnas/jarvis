import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { BrowserCaptureCard } from "./BrowserCaptureCard";
import { ToolApproval } from "./ToolApproval";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => "data:image/png;base64,dGVzdA==") }));

it("shows and enlarges an actual captured attachment from the tool result", async () => {
  const user = userEvent.setup();
  render(<BrowserCaptureCard tool={{ id: "capture-tool", name: "browser_screenshot", status: "completed", output: JSON.stringify({ attachment: { id: "capture-1", conversationId: "chat-1", name: "navegador.png", mime: "image/png", kind: "image", size: 256 }, url: "http://localhost:3000/" }) }} />);
  await user.click(screen.getByRole("button", { name: "Ampliar captura do navegador" }));
  expect(await screen.findByRole("dialog")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Salvar captura" }));
  expect(invoke).toHaveBeenCalledWith("save_chat_image", { conversationId: "chat-1", id: "capture-1" });
});

it("asks approval for a browser action using its actual target URL", async () => {
  const answer = vi.fn(async () => true);
  render(<ToolApproval tool={{ id: "open-1", name: "browser_open", args: { url: "http://localhost:5173/" }, status: "pending", output: "", durationMs: 0 }} projectPath="C:\\projeto" onAnswer={answer} />);
  expect(screen.getByText("Autorizar ação no navegador?")).toBeVisible();
  expect(screen.getByText("http://localhost:5173/")).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Recusar" }));
  expect(answer).toHaveBeenCalledWith(false);
});
