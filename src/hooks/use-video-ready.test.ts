import { renderHook, act } from "@testing-library/react";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import type { ProjectFilesController } from "./use-project-files";
import { useVideoReady } from "./use-video-ready";

let listener: EventCallback<unknown> | undefined;
const unlisten = vi.fn();
function files(projectId = "project-1"): ProjectFilesController {
  return { projectId, tabs: { activePath: null, paths: [] }, active: { loading: false }, open: vi.fn(), close: vi.fn(), select: vi.fn(), refresh: vi.fn(), presentedVideos: new Set<string>() };
}
beforeEach(() => {
  listener = undefined; unlisten.mockClear();
  vi.mocked(listen).mockImplementation(async (_, handler) => { listener = handler; return unlisten; });
});
async function ready(payload: unknown) { await act(async () => { listener?.({ event: "video:ready", id: 1, payload }); }); }

it("recovers verified OpenMontage videos while rejecting incomplete outputs and other media", () => {
  const controller = files();
  const turn = savedTurn();
  const receipt = { sessionId: "production-1", resource: "openmontage", action: "tool", status: "completed", exitCode: 0, result: { success: true, videos: [{ path: "renders/demo.mp4", verified: true }, { path: "renders/unverified.mp4", verified: false }, { path: "audio/voice.wav", verified: true }, { path: "renders/demo.mp4", verified: true }] } };
  turn.steps[0].tools[0] = { ...turn.steps[0].tools[0], name: "video_run", output: JSON.stringify({ ...receipt, exitCode: 1 }) };
  const snapshot = { ...emptyChat("chat-1"), turns: [turn] };
  const { rerender } = renderHook(({ data }) => useVideoReady("project-1", "chat-1", controller, data), { initialProps: { data: snapshot } });
  expect(controller.open).not.toHaveBeenCalled();
  turn.steps[0].tools[0].output = JSON.stringify({ ...receipt, result: { ...receipt.result, success: false } });
  rerender({ data: { ...snapshot, revision: 2 } });
  expect(controller.open).not.toHaveBeenCalled();
  turn.steps[0].tools[0].output = JSON.stringify(receipt);
  rerender({ data: { ...snapshot, revision: 3 } });
  expect(controller.open).toHaveBeenCalledExactlyOnceWith("renders/demo.mp4", true);
  turn.steps[0].tools.push({ ...turn.steps[0].tools[0], id: "wait-1", name: "video_wait" });
  rerender({ data: { ...snapshot, revision: 4 } });
  expect(controller.open).toHaveBeenCalledTimes(1);
});

it("only opens a completed render from the current project and conversation", async () => {
  const controller = files();
  const { unmount } = renderHook(() => useVideoReady("project-1", "chat-1", controller));
  await ready({ projectId: "project-2", conversationId: "chat-1", path: "render.mp4" });
  await ready({ projectId: "project-1", conversationId: "chat-2", path: "render.mp4" });
  await ready({ projectId: "project-1", conversationId: "chat-1", path: "render.txt" });
  await ready({ projectId: "project-1", conversationId: "chat-1", path: 1 });
  expect(controller.open).not.toHaveBeenCalled();
  await ready({ projectId: "project-1", conversationId: "chat-1", path: "renders/demo.mp4", title: "Vídeo pronto" });
  expect(controller.open).toHaveBeenCalledWith("renders/demo.mp4", true);
  unmount();
  await ready({ projectId: "project-1", conversationId: "chat-1", path: "late.mp4" });
  expect(controller.open).toHaveBeenCalledTimes(1);
  expect(unlisten).toHaveBeenCalledOnce();
});

it("does not subscribe when the file controller belongs to a different project", () => {
  vi.mocked(listen).mockClear();
  renderHook(() => useVideoReady("project-1", "chat-1", files("project-2")));
  expect(listen).not.toHaveBeenCalled();
});

it("recovers successful render receipts once per output while ignoring running and failed receipts", () => {
  const controller = files();
  const turn = savedTurn();
  const receipt = { sessionId: "render-1", action: "render", path: "renders/demo.mp4", status: "running", exitCode: null };
  const tool = { ...turn.steps[0].tools[0], name: "video_run", output: JSON.stringify(receipt) };
  turn.steps[0].tools = [tool];
  const snapshot = { ...emptyChat("chat-1"), turns: [turn] };
  const { rerender } = renderHook(({ data }) => useVideoReady("project-1", "chat-1", controller, data), { initialProps: { data: snapshot } });
  expect(controller.open).not.toHaveBeenCalled();
  tool.output = JSON.stringify({ ...receipt, status: "completed", exitCode: 1 });
  rerender({ data: { ...snapshot, revision: 2 } });
  expect(controller.open).not.toHaveBeenCalled();
  tool.output = JSON.stringify({ ...receipt, status: "completed", exitCode: 0 });
  rerender({ data: { ...snapshot, revision: 3 } });
  expect(controller.open).toHaveBeenCalledExactlyOnceWith("renders/demo.mp4", true);
  turn.steps[0].tools.push({ ...tool, id: "wait-1", name: "bash_wait" });
  rerender({ data: { ...snapshot, revision: 4 } });
  expect(controller.open).toHaveBeenCalledTimes(1);
});

it("does not reopen a closed video when its receipt follows the completion event", async () => {
  const controller = files();
  const snapshot = emptyChat("chat-1");
  const { rerender } = renderHook(({ data }) => useVideoReady("project-1", "chat-1", controller, data), { initialProps: { data: snapshot } });
  await ready({ projectId: "project-1", conversationId: "chat-1", path: "renders/demo.mp4" });
  expect(controller.open).toHaveBeenCalledTimes(1);
  const turn = savedTurn();
  turn.steps[0].tools[0] = { ...turn.steps[0].tools[0], name: "bash_wait", output: JSON.stringify({ sessionId: "render-1", action: "render", path: "renders/demo.mp4", status: "completed", exitCode: 0 }) };
  rerender({ data: { ...snapshot, turns: [turn], revision: 2 } });
  await ready({ projectId: "project-1", conversationId: "chat-1", path: "renders/demo.mp4" });
  expect(controller.open).toHaveBeenCalledTimes(1);
});

it("keeps a closed video closed after returning to the chat and still presents new renders", async () => {
  const controller = files();
  const turn = savedTurn();
  const receipt = { sessionId: "render-1", action: "render", path: "renders/demo.mp4", status: "completed", exitCode: 0 };
  turn.steps[0].tools[0] = { ...turn.steps[0].tools[0], name: "video_wait", output: JSON.stringify(receipt) };
  const snapshot = { ...emptyChat("chat-1"), turns: [turn] };
  const first = renderHook(() => useVideoReady("project-1", "chat-1", controller, snapshot));
  expect(controller.open).toHaveBeenCalledExactlyOnceWith("renders/demo.mp4", true);
  controller.close("renders/demo.mp4");
  first.unmount();

  const second = renderHook(() => useVideoReady("project-1", "chat-1", controller, snapshot));
  await ready({ projectId: "project-1", conversationId: "chat-1", path: "renders/demo.mp4" });
  expect(controller.open).toHaveBeenCalledTimes(1);
  await ready({ projectId: "project-1", conversationId: "chat-1", path: "renders/new.mp4" });
  expect(controller.open).toHaveBeenCalledTimes(2);
  expect(controller.open).toHaveBeenLastCalledWith("renders/new.mp4", true);
  second.unmount();
});
