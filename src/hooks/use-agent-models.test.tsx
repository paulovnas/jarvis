import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { useAgentModels, type AgentModelConfig } from "./use-agent-models";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
beforeEach(() => vi.clearAllMocks());

it("saves, reloads and removes the native agent secondary model", async () => {
  const primary = { account: "work", model: "primary", reasoning: null };
  const choice = { ...primary, fallback: { account: "personal", model: "secondary", reasoning: "high" } };
  let stored: AgentModelConfig = { "standard/builder": primary };
  vi.mocked(invoke).mockImplementation(async command => command === "set_agent_model" ? (stored = { "standard/builder": choice }) : stored);
  const first = renderHook(() => useAgentModels());
  await waitFor(() => expect(first.result.current.data).toEqual(stored));
  expect(first.result.current.data?.["standard/builder"].fallback).toBeUndefined();
  await act(async () => { expect(await first.result.current.save("standard", "builder", choice)).toBe(true); });
  expect(invoke).toHaveBeenCalledWith("set_agent_model", { flow: "standard", role: "builder", choice });
  first.unmount();
  const second = renderHook(() => useAgentModels());
  await waitFor(() => expect(second.result.current.data?.["standard/builder"]).toEqual(choice));
  vi.mocked(invoke).mockImplementation(async () => (stored = { "standard/builder": { ...primary, fallback: null } }));
  await act(async () => { expect(await second.result.current.save("standard", "builder", { ...primary, fallback: null })).toBe(true); });
  expect(second.result.current.data?.["standard/builder"].fallback).toBeNull();
  second.unmount();
  const third = renderHook(() => useAgentModels());
  await waitFor(() => expect(third.result.current.data?.["standard/builder"].fallback).toBeNull());
});

it("rejects selecting the secondary target as primary even with different reasoning", async () => {
  const primary = { account: "work", model: "primary", reasoning: null };
  vi.mocked(invoke).mockResolvedValue({ "standard/builder": primary });
  const { result } = renderHook(() => useAgentModels());
  await waitFor(() => expect(result.current.data).not.toBeNull());
  await act(async () => { expect(await result.current.save("standard", "builder", { ...primary, fallback: { ...primary, reasoning: "high" } })).toBe(false); });
  expect(invoke).not.toHaveBeenCalledWith("set_agent_model", expect.anything());
  expect(toast.error).toHaveBeenCalledWith("Escolha um modelo secundário diferente do principal.");
});
