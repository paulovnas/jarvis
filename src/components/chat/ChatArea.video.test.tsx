import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { populatedLibrary } from "@/test/library-fixtures";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { clearChatStore } from "@/core/chat-store";
import { readChat, type ChatSnapshot } from "@/core/chat";
import { useChat } from "@/hooks/use-chat";
import { useProjectFiles } from "@/hooks/use-project-files";
import { ChatArea } from "./ChatArea";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}` }));
const listeners = new Map<string, Set<EventCallback<unknown>>>();
let snapshot: ChatSnapshot;
function VideoChat() {
  const files = useProjectFiles("p1");
  return <ChatArea library={populatedLibrary()} chat={useChat("c1")} files={files} />;
}
async function emit(name: string, payload: unknown) { await act(async () => { listeners.get(name)?.forEach(handler => handler({ event: name, id: 1, payload })); }); }
beforeEach(() => {
  clearChatStore(); listeners.clear(); snapshot = emptyChat();
  vi.mocked(listen).mockImplementation(async (name, handler) => { const set = listeners.get(name) ?? new Set(); set.add(handler); listeners.set(name, set); return () => { set.delete(handler); }; });
  vi.mocked(invoke).mockReset().mockImplementation(async (command, args) => {
    if (command === "get_project_video") return { path: (args as { path: string }).path, absolutePath: "/projects/jarvis/render.mp4", size: 2048, mime: "video/mp4" };
    if (command === "get_browser_tabs") return { tabs: [], activeId: null };
    if (command === "get_http_snapshot") return { collections: [], drafts: [], runs: [] };
    return snapshot;
  });
});

it("shows the confirmed generated video in a chat tab and returns to Chat for approval", async () => {
  render(<VideoChat />);
  await screen.findByRole("textbox");
  await emit("video:ready", { projectId: "p2", conversationId: "c1", path: "render.mp4" });
  expect(screen.queryByRole("tab", { name: "render.mp4" })).not.toBeInTheDocument();
  await emit("video:ready", { projectId: "p1", conversationId: "c1", path: "render.mp4" });
  expect(await screen.findByLabelText("Vídeo render.mp4")).toHaveAttribute("controls");
  expect(screen.getByRole("tab", { name: "render.mp4" })).toHaveAttribute("aria-selected", "true");
  const pendingApproval = { tool: { id: "approval", name: "write", args: { path: "index.html", content: "content" }, status: "pending" as const, output: "", durationMs: 0 }, policy: null };
  const next = readChat({ ...emptyChat(), revision: 2, pendingApproval }, "c1");
  await emit("agent:event", { conversationId: "c1", baseRevision: 1, revision: 2, events: [{ type: "stateChanged", state: { ...next, pendingAuthoring: null } }] });
  expect(await screen.findByRole("button", { name: "Autorizar uma vez" })).toBeVisible();
  expect(screen.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tab", { name: "render.mp4" })).toBeEnabled();
});

it("recovers a missed completed render event once and respects a later closed tab", async () => {
  const user = userEvent.setup();
  const turn = savedTurn();
  turn.steps[0].tools = [{ ...turn.steps[0].tools[0], name: "video_wait", output: JSON.stringify({ sessionId: "render-1", status: "completed", exitCode: 0, action: "render", path: "render.mp4" }) }];
  snapshot = { ...emptyChat(), turns: [turn] };
  render(<VideoChat />);
  expect(await screen.findByLabelText("Vídeo render.mp4")).toHaveAttribute("controls");
  await user.click(screen.getByRole("button", { name: "Fechar arquivo render.mp4" }));
  expect(screen.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
  await emit("agent:event", { conversationId: "c1", baseRevision: 1, revision: 2, events: [{ type: "stateChanged", state: { ...readChat({ ...snapshot, revision: 2 }, "c1"), pendingAuthoring: null } }] });
  await waitFor(() => expect(screen.queryByRole("tab", { name: "render.mp4" })).not.toBeInTheDocument());
});
