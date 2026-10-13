import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { BrowserCaptureCard } from "./BrowserCaptureCard";
import { ToolApproval } from "./ToolApproval";

it("shows an unavailable native observation without a capture or image lookup", () => {
  vi.mocked(invoke).mockClear();
  render(<BrowserCaptureCard tool={{ id: "native-unavailable", name: "computer_observe", status: "completed", output: JSON.stringify({ targetId: "safari-window", available: false, code: "computer_target_unavailable", phase: "observed", inputSent: false }) }} />);
  expect(screen.getByRole("status", { name: "Observação do aplicativo indisponível" })).toHaveTextContent("A janela autorizada não está disponível. Nenhuma captura foi obtida.");
  expect(screen.queryByRole("button", { name: "Ampliar captura do aplicativo" })).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalled();
});

it("shows and saves a native window capture without treating its title as a browser URL", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockImplementation(async command => command === "save_chat_image" ? true : "data:image/png;base64,dGVzdA==");
  render(<BrowserCaptureCard tool={{ id: "native-capture", name: "computer_observe", status: "completed", output: JSON.stringify({ observation: { window: { title: "Safari · Documentação" } }, attachment: { id: "native-1", conversationId: "chat-1", name: "aplicativo.png", mime: "image/png", kind: "image", size: 256 }, untrusted: true }) }} />);
  expect(screen.getByRole("region", { name: "Captura do aplicativo" })).toBeVisible();
  expect(screen.getByText("Safari · Documentação")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Ampliar captura do aplicativo" }));
  await user.click(screen.getByRole("button", { name: "Salvar captura" }));
  expect(invoke).toHaveBeenCalledWith("save_chat_image", { conversationId: "chat-1", id: "native-1" });
});
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
  render(<ToolApproval request={{ tool: { id: "open-1", name: "browser_open", args: { url: "http://localhost:5173/" }, status: "pending", output: "", durationMs: 0 }, policy: null }} projectPath="C:\\projeto" onAnswer={answer} />);
  expect(screen.getByText("Autorizar ação no navegador?")).toBeVisible();
  expect(screen.getByText("http://localhost:5173/")).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Recusar" }));
  expect(answer).toHaveBeenCalledWith({ approved: false, grant: null });
});
