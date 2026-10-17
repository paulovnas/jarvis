import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { emptyChat, savedTurn } from "@/test/chat-fixtures";
import { useMessagePrediction } from "./use-message-prediction";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
beforeEach(() => { vi.mocked(listen).mockResolvedValue(() => {}); vi.mocked(invoke).mockReset().mockResolvedValue({ turnId: "turn1", text: "Pode mostrar os testes?" }); });

it("hides an inactive editor without dismissing its delivery and restores the cached preview on return", async () => {
  const snapshot = { ...emptyChat(), turns: [savedTurn()] };
  const { result, rerender } = renderHook(({ paused }) => useMessagePrediction("c1", snapshot, false, paused), { initialProps: { paused: false } });
  await waitFor(() => expect(result.current.text).toBe("Pode mostrar os testes?"));
  rerender({ paused: true });
  expect(result.current.text).toBeUndefined();
  expect(invoke).not.toHaveBeenCalledWith("dismiss_chat_prediction", expect.anything());
  rerender({ paused: false });
  await waitFor(() => expect(result.current.text).toBe("Pode mostrar os testes?"));
});

it("discards a preview when an external draft arrives even if dismissal has not returned before clearing it", async () => {
  const snapshot = { ...emptyChat(), turns: [savedTurn()] };
  const { result, rerender } = renderHook(({ occupied }) => useMessagePrediction("c1", snapshot, occupied), { initialProps: { occupied: false } });
  await waitFor(() => expect(result.current.text).toBe("Pode mostrar os testes?"));
  vi.mocked(invoke).mockImplementation(async command => command === "dismiss_chat_prediction" ? new Promise(() => {}) : { turnId: "turn1", text: "Pode mostrar os testes?" });
  rerender({ occupied: true });
  expect(result.current.text).toBeUndefined();
  rerender({ occupied: false });
  expect(result.current.text).toBeUndefined();
});

it("refetches a prediction that finished while the chat was compacting", async () => {
  const snapshot = { ...emptyChat(), turns: [savedTurn()] };
  vi.mocked(invoke).mockResolvedValueOnce(null);
  const { result, rerender } = renderHook(({ compacting }) => useMessagePrediction("c1", { ...snapshot, compacting }, false, compacting), { initialProps: { compacting: false } });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_chat_prediction", { conversationId: "c1" }));
  rerender({ compacting: true });
  expect(result.current.text).toBeUndefined();
  expect(invoke).not.toHaveBeenCalledWith("dismiss_chat_prediction", expect.anything());
  rerender({ compacting: false });
  await waitFor(() => expect(result.current.text).toBe("Pode mostrar os testes?"));
});

it("does not let a read started before compaction erase the recovered prediction", async () => {
  const snapshot = { ...emptyChat(), turns: [savedTurn()] };
  let resolveOld: ((value: null) => void) | undefined;
  vi.mocked(invoke).mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; }));
  const { result, rerender } = renderHook(({ compacting }) => useMessagePrediction("c1", { ...snapshot, compacting }, false, compacting), { initialProps: { compacting: false } });
  await waitFor(() => expect(resolveOld).toBeDefined());
  rerender({ compacting: true });
  rerender({ compacting: false });
  await waitFor(() => expect(result.current.text).toBe("Pode mostrar os testes?"));
  await act(async () => { resolveOld?.(null); });
  expect(result.current.text).toBe("Pode mostrar os testes?");
});
