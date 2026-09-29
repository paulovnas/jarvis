import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { LearningSnapshot, ProjectLesson } from "@/core/project-learning";
import type { ChatSnapshot } from "@/core/chat";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { useChatLearning } from "./use-chat-learning";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const call = vi.mocked(invoke);
const lesson: ProjectLesson = {
  id: "select", scope: ".", content: "Use o label no Select.", topics: [], check: "",
  status: "active", origin: "feedback", revision: 1, updatedAt: 1,
  evidence: [{ conversationId: "c1", messageId: "turn1", excerpt: "Sempre exiba o label do Select", createdAt: 1 }],
};
const saved = (lessons: ProjectLesson[] = []): LearningSnapshot => ({ enabled: true, revision: 1, lessons, pending: 0, notice: null });
let changed: EventCallback<string>;
const stop = vi.fn();
beforeEach(() => {
  stop.mockReset(); call.mockReset().mockResolvedValue(saved());
  vi.mocked(listen).mockReset().mockImplementation(async (_event, callback) => { changed = callback as EventCallback<string>; return stop; });
});

it("loads persisted lessons, reacts to background saves and releases the subscription", async () => {
  const { result, unmount } = renderHook(() => useChatLearning("p1", emptyChat()));
  await waitFor(() => expect(call).toHaveBeenCalledWith("get_project_learning", { projectId: "p1" }));
  expect(result.current).toEqual([]);
  call.mockResolvedValue(saved([lesson, { ...lesson, id: "other", evidence: [{ ...lesson.evidence[0], conversationId: "c2" }] }]));
  await act(async () => changed({ event: "project:learning-changed", id: 1, payload: "p2" }));
  expect(call).toHaveBeenCalledTimes(1);
  await act(async () => changed({ event: "project:learning-changed", id: 2, payload: "p1" }));
  expect(result.current).toEqual([lesson]);
  unmount();
  await waitFor(() => expect(stop).toHaveBeenCalledOnce());
  const reopened = renderHook(() => useChatLearning("p1", emptyChat()));
  await waitFor(() => expect(reopened.result.current).toEqual([lesson]));
});

it("refreshes on completed learning tools and turn completion, not every streaming revision", async () => {
  const turn = savedTurn();
  turn.status = "running";
  const snapshot: ChatSnapshot = { ...emptyChat(), activeTurnId: turn.id, turns: [turn] };
  const { result, rerender } = renderHook(({ chat }) => useChatLearning("p1", chat), { initialProps: { chat: snapshot } });
  await waitFor(() => expect(call).toHaveBeenCalledTimes(1));
  rerender({ chat: { ...snapshot, revision: 99, turns: [{ ...turn, steps: [{ ...turn.steps[0], text: "Ainda trabalhando" }] }] } });
  expect(call).toHaveBeenCalledTimes(1);
  call.mockResolvedValue(saved([lesson]));
  const recorded = { ...snapshot, turns: [{ ...turn, steps: [{ ...turn.steps[0], tools: [{ ...turn.steps[0].tools[0], id: "learned", name: "learn_project" }] }] }] };
  rerender({ chat: recorded });
  await waitFor(() => expect(result.current).toEqual([lesson]));
  expect(call).toHaveBeenCalledTimes(2);
  rerender({ chat: { ...recorded, activeTurnId: null } });
  await waitFor(() => expect(call).toHaveBeenCalledTimes(3));
});

it("ignores stale loads after switching projects or conversations and preserves data on transient errors", async () => {
  let finishOld: (value: LearningSnapshot) => void = () => {};
  call.mockReturnValueOnce(new Promise<LearningSnapshot>(resolve => { finishOld = resolve; }));
  const { result, rerender } = renderHook(({ project, chat }) => useChatLearning(project, emptyChat(chat)), { initialProps: { project: "p1", chat: "c1" } });
  await waitFor(() => expect(call).toHaveBeenCalledTimes(1));
  const current = { ...lesson, id: "current", evidence: [{ ...lesson.evidence[0], conversationId: "c2" }] };
  call.mockResolvedValue(saved([current]));
  rerender({ project: "p2", chat: "c2" });
  expect(result.current).toEqual([]);
  await waitFor(() => expect(result.current).toEqual([current]));
  await act(async () => finishOld(saved([lesson])));
  expect(result.current).toEqual([current]);
  call.mockRejectedValueOnce(new Error("Temporary storage error"));
  await act(async () => changed({ event: "project:learning-changed", id: 1, payload: "p2" }));
  expect(result.current).toEqual([current]);
  call.mockResolvedValue(saved());
  await act(async () => changed({ event: "project:learning-changed", id: 2, payload: "p2" }));
  expect(result.current).toEqual([]);
});
