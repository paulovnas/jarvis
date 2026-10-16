import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import type { MuseRuntime } from "@/core/executors";
import { WebSearchSettings } from "./WebSearchSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), dismiss: vi.fn() } }));
const runtime = vi.hoisted(() => ({ data: null as MuseRuntime | null, loading: false, error: null as string | null }));
vi.mock("@/hooks/use-muse-runtime", () => ({ useMuseRuntime: () => runtime }));
beforeEach(() => {
  runtime.data = { installed: true, authenticated: true, version: "1", error: null, models: ["muse-wonder", "muse-spark"].map(id => ({ id, name: id, description: "", reasoningLevels: [], defaultReasoning: null })) };
  runtime.error = null; runtime.loading = false;
  vi.mocked(invoke).mockReset().mockImplementation(async (command, args) => command === "get_web_search_config" ? { executor: "jarvis", accountAlias: null, model: null, inheritChat: true } : args);
});

it("uses the Muse subscription for search independently of the chat model without an API alias", async () => {
  const user = userEvent.setup();
  render(<WebSearchSettings accounts={[]} />);
  const provider = await screen.findByRole("combobox", { name: "Provedor de Web Search" });
  await user.click(provider);
  await user.click(await screen.findByRole("option", { name: "Muse Code" }));
  await waitFor(() => expect(provider).toHaveTextContent("Muse Code"));
  expect(invoke).toHaveBeenCalledWith("set_web_search_config", { executor: "muse", accountAlias: null, model: "muse-wonder", inheritChat: false });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByText(/independente do modelo do chat/)).toBeVisible();
  await user.click(screen.getByRole("combobox", { name: "Modelo de Web Search" }));
  await user.click(await screen.findByRole("option", { name: "muse-spark" }));
  expect(invoke).toHaveBeenLastCalledWith("set_web_search_config", { executor: "muse", accountAlias: null, model: "muse-spark", inheritChat: false });
  await user.click(provider);
  await user.click(await screen.findByRole("option", { name: "Herdar do chat" }));
  await waitFor(() => expect(provider).toHaveTextContent("Herdar do chat"));
  expect(invoke).toHaveBeenLastCalledWith("set_web_search_config", { accountAlias: null, model: null, inheritChat: true });
});

it("preserves saved Muse search and explains an unavailable subscription without choosing an API account", async () => {
  vi.mocked(invoke).mockResolvedValue({ executor: "muse", accountAlias: null, model: "muse-wonder", inheritChat: false });
  runtime.data!.authenticated = false;
  const view = render(<WebSearchSettings accounts={[]} />);
  expect(await screen.findByRole("combobox", { name: "Provedor de Web Search" })).toHaveTextContent("Muse Code");
  expect(screen.getByRole("alert")).toHaveTextContent("Conecte sua assinatura Muse Code");
  expect(invoke).not.toHaveBeenCalledWith("set_web_search_config", expect.anything());
  runtime.data = { ...runtime.data!, authenticated: true };
  act(() => { view.rerender(<WebSearchSettings accounts={[]} />); });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "Modelo de Web Search" })).toHaveTextContent("muse-wonder");
});

it.each(["vision", "image_generation"] as const)("keeps Muse search separate from %s", async kind => {
  vi.mocked(invoke).mockResolvedValue({ accountAlias: null, model: null, inheritChat: false });
  const user = userEvent.setup();
  render(<WebSearchSettings accounts={[]} kind={kind} />);
  await user.click(await screen.findByRole("combobox", { name: kind === "vision" ? "Provedor de Vision" : "Provedor de Gerar imagens" }));
  expect(screen.queryByRole("option", { name: "Muse Code" })).not.toBeInTheDocument();
});
