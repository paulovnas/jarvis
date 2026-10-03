import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { ProviderAccount } from "@/core/provider-accounts";
import { OpenCodeGoProviderForm } from "./OpenCodeGoProviderForm";

const { call } = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: call }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const account: ProviderAccount = { alias: "opencode-go-pessoal", providerKind: "opencode-go", enabled: true, showUsage: true, createdAt: 1, email: null, accountType: "personal", modelsAvailable: true, models: [{ id: "glm", name: "GLM", reasoningLevels: [], defaultReasoningLevel: null }] };
beforeEach(() => { call.mockReset().mockResolvedValue(account); });

it("connects a subscription by API key and returns discovered models without exposing the key", async () => {
  const saved = vi.fn(); const user = userEvent.setup();
  render(<OpenCodeGoProviderForm onSaved={saved} onCancel={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  const key = screen.getByLabelText("Chave de API");
  expect(key).toHaveAttribute("type", "password");
  fireEvent.change(key, { target: { value: " private-test-key " } });
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  await waitFor(() => expect(saved).toHaveBeenCalledWith(account));
  expect(call).toHaveBeenCalledExactlyOnceWith("save_opencode_go_provider", { alias: account.alias, apiKey: "private-test-key", editing: false });
  expect(key).toHaveValue("");
  expect(screen.queryByText("private-test-key")).not.toBeInTheDocument();
});

it("requires a valid alias and a key before connecting", async () => {
  const user = userEvent.setup();
  render(<OpenCodeGoProviderForm onSaved={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  expect(screen.getByRole("alert")).toHaveTextContent("sufixo");
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  await user.click(screen.getByRole("button", { name: "Conectar OpenCode Go" }));
  expect(screen.getByRole("alert")).toHaveTextContent("chave de API");
  expect(call).not.toHaveBeenCalled();
});

it.each([null, "replacement-test-key"])("keeps the account identity while replacing or preserving its key: %s", async apiKey => {
  const user = userEvent.setup();
  render(<OpenCodeGoProviderForm account={account} onSaved={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.getByLabelText("Sufixo do alias")).toBeDisabled();
  if (apiKey) fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: apiKey } });
  await user.click(screen.getByRole("button", { name: "Salvar alterações" }));
  await waitFor(() => expect(call).toHaveBeenCalledExactlyOnceWith("save_opencode_go_provider", { alias: account.alias, apiKey, editing: true }));
});

it("locks duplicate submissions and keeps a failed connection editable without showing its secret", async () => {
  let reject!: (reason: Error) => void;
  call.mockImplementation(() => new Promise<unknown>((_resolve, rejectPromise) => { reject = rejectPromise; }));
  const busy = vi.fn(); const saved = vi.fn();
  const { container } = render(<OpenCodeGoProviderForm onSaved={saved} onCancel={vi.fn()} onBusyChange={busy} />);
  fireEvent.change(screen.getByLabelText("Sufixo do alias"), { target: { value: "pessoal" } });
  fireEvent.change(screen.getByLabelText("Chave de API"), { target: { value: "private-test-key" } });
  fireEvent.submit(container.querySelector("form")!); fireEvent.submit(container.querySelector("form")!);
  expect(call).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Conectando…" })).toBeDisabled();
  await act(async () => reject(new Error("Chave private-test-key não aceita.")));
  expect(screen.getByRole("alert")).toHaveTextContent("[chave ocultada]");
  expect(screen.getByRole("alert")).not.toHaveTextContent("private-test-key");
  expect(screen.getByRole("button", { name: "Conectar OpenCode Go" })).toBeEnabled();
  expect(busy.mock.calls).toEqual([[true], [false]]);
  expect(saved).not.toHaveBeenCalled();
});
