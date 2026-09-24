import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ProviderTransportSettings } from "./ProviderTransportSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

it("loads the account preference and saves an explicit opt-in", async () => {
  vi.mocked(invoke).mockResolvedValueOnce({ supported: true, enabled: false }).mockResolvedValueOnce({ supported: true, enabled: true });
  render(<ProviderTransportSettings alias="codex-work" />);
  expect(screen.getByRole("status", { name: "Carregando opção de conexão" })).toBeVisible();
  const toggle = await screen.findByRole("switch", { name: "Conexão incremental · experimental" });
  expect(toggle).not.toBeChecked();
  fireEvent.click(toggle);
  await waitFor(() => expect(toggle).toBeChecked());
  expect(invoke).toHaveBeenCalledWith("set_provider_transport", { alias: "codex-work", enabled: true });
});

it("does not show the experimental switch for an unsupported protocol", async () => {
  vi.mocked(invoke).mockResolvedValue({ supported: false, enabled: false });
  render(<ProviderTransportSettings alias="gemini" />);
  await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
  expect(screen.queryByRole("switch")).not.toBeInTheDocument();
});

it("preserves the saved value if saving fails", async () => {
  vi.mocked(invoke).mockResolvedValueOnce({ supported: true, enabled: false }).mockRejectedValueOnce(new Error("offline"));
  render(<ProviderTransportSettings alias="codex" />);
  const toggle = await screen.findByRole("switch");
  fireEvent.click(toggle);
  await waitFor(() => expect(toggle).not.toBeDisabled());
  expect(toggle).not.toBeChecked();
});

it("loads each account independently when switching providers", async () => {
  vi.mocked(invoke).mockResolvedValueOnce({ supported: true, enabled: true }).mockResolvedValueOnce({ supported: true, enabled: false });
  const { rerender } = render(<ProviderTransportSettings alias="work" />);
  expect(await screen.findByRole("switch")).toBeChecked();
  rerender(<ProviderTransportSettings alias="personal" />);
  expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  expect(await screen.findByRole("switch")).not.toBeChecked();
  expect(invoke).toHaveBeenLastCalledWith("get_provider_transport", { alias: "personal" });
});
