import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { providerReference, referenceAccount } from "@/test/provider-reference-fixtures";
import { useProviderReferences } from "./use-provider-references";
import type { ProviderAccount } from "@/core/provider-accounts";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), dismiss: vi.fn() } }));
const call = vi.mocked(invoke); const listeners = new Map<string, EventCallback<unknown>>();
const item = providerReference(); const accounts = [referenceAccount()];
beforeEach(() => {
  vi.clearAllMocks(); listeners.clear(); call.mockResolvedValue({ references: [item], bindings: [] });
  vi.mocked(listen).mockImplementation(async (name, callback) => { listeners.set(name, callback); return () => { listeners.delete(name); }; });
});

it("waits for accounts, reports missing references once and clears the notice after repair", async () => {
  const { result, rerender, unmount } = renderHook(({ ready }) => useProviderReferences(accounts, ready), { initialProps: { ready: false } });
  expect(call).not.toHaveBeenCalled(); expect(toast.error).not.toHaveBeenCalled();
  rerender({ ready: true }); await waitFor(() => expect(result.current.loading).toBe(false));
  expect(toast.error).toHaveBeenCalledWith("1 configuração precisa de outro provedor ou modelo", expect.objectContaining({ description: expect.stringContaining("Analista") }));
  await act(async () => { listeners.get("agent-models:changed")?.({ event: "agent-models:changed", id: 1, payload: null }); });
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(toast.error).toHaveBeenCalledTimes(1);
  const target = { account: "novo", model: "gpt-test", reasoning: "high" };
  call.mockResolvedValue({ references: [{ ...item, choice: target }], bindings: [{ itemKey: "chat:c1", source: item.choice, target }] });
  await act(async () => { listeners.get("provider-model-bindings:changed")?.({ event: "provider-model-bindings:changed", id: 2, payload: null }); });
  await waitFor(() => expect(result.current.bindings[0]?.target).toEqual(target));
  expect(toast.dismiss).toHaveBeenCalledWith("provider-invalid-models");
  unmount(); expect(listeners.size).toBe(0);
});

it("does not combine stale references with a new account list", async () => {
  const old: ProviderAccount = { ...referenceAccount("antigo"), models: [{ id: "modelo-anterior", name: "Anterior", reasoningLevels: [], defaultReasoningLevel: null }] };
  const { result, rerender } = renderHook(({ providers }) => useProviderReferences(providers, true), { initialProps: { providers: [old] } });
  await waitFor(() => expect(result.current.loading).toBe(false));
  let finish!: (value: unknown) => void; call.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  rerender({ providers: accounts });
  expect(result.current.loading).toBe(true); expect(toast.error).not.toHaveBeenCalled();
  await waitFor(() => expect(call).toHaveBeenCalledTimes(2));
  await act(async () => { finish({ references: [], bindings: [] }); });
  expect(result.current.loading).toBe(false); expect(toast.error).not.toHaveBeenCalled();
});
