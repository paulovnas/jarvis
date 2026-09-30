import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { APP_VERSION } from "@/core/app-update";
import { DEFAULT_DESKTOP_LAYOUT, type DesktopLayout } from "@/core/desktop-layout";
import { DesktopLayoutProvider } from "./DesktopLayoutProvider";
import { InstalledReleaseDialog } from "./InstalledReleaseDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

const release = { version: APP_VERSION, notes: "## Novos recursos\n\n- Integração com o navegador.\n\n## Correções\n\nAs conversas preservam seu histórico." };
let saved: DesktopLayout;
const start = () => render(<DesktopLayoutProvider><InstalledReleaseDialog /></DesktopLayoutProvider>);

beforeEach(() => {
  saved = { ...DEFAULT_DESKTOP_LAYOUT };
  vi.stubGlobal("__JARVIS_INSTALLED_RELEASE__", release);
  vi.mocked(invoke).mockReset().mockImplementation(async (command, args) => {
    if (command === "get_desktop_layout") return saved;
    if (command === "save_desktop_layout") { saved = (args as { layout: DesktopLayout }).layout; return; }
    throw new Error(`Unexpected command ${command}`);
  });
});
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

it("shows the full installed changelog offline after restoring preferences", async () => {
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(false);
  start();
  expect(await screen.findByRole("dialog", { name: "Novidades do Jarvis" })).toBeVisible();
  expect(screen.getByText(APP_VERSION)).toBeVisible();
  expect(await screen.findByRole("heading", { name: "Novos recursos" })).toBeVisible();
  expect(screen.getByText("Integração com o navegador.")).toBeVisible();
  expect(screen.getByText("As conversas preservam seu histórico.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Continuar" })).toBeVisible();
  expect(saved.lastSeenReleaseVersion).toBeNull();
  expect(invoke).toHaveBeenCalledTimes(1);
});

it("acknowledges dismissal and does not repeat the dialog after restart", async () => {
  const user = userEvent.setup();
  const first = start();
  await user.click(await screen.findByRole("button", { name: "Continuar" }));
  await waitFor(() => expect(saved.lastSeenReleaseVersion).toBe(APP_VERSION));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  first.unmount();
  start();
  await waitFor(() => expect(screen.queryByRole("status", { name: "Carregando Jarvis" })).not.toBeInTheDocument());
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("shows a new installed version and acknowledges closing with Escape", async () => {
  saved.lastSeenReleaseVersion = "1.0.0-beta.1";
  const user = userEvent.setup();
  start();
  await screen.findByRole("dialog", { name: "Novidades do Jarvis" });
  await user.keyboard("{Escape}");
  await waitFor(() => expect(saved.lastSeenReleaseVersion).toBe(APP_VERSION));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
});

it.each([
  null,
  { version: "999.0.0", notes: "Novidades de uma versão ainda não instalada." },
  { version: APP_VERSION, notes: " \n " },
])("ignores unavailable, mismatched or empty bundled release notes: %j", async metadata => {
  vi.stubGlobal("__JARVIS_INSTALLED_RELEASE__", metadata);
  start();
  await waitFor(() => expect(screen.queryByRole("status", { name: "Carregando Jarvis" })).not.toBeInTheDocument());
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(saved.lastSeenReleaseVersion).toBeNull();
});

it("does not flash the dialog while acknowledged preferences are still loading", async () => {
  let restore!: (layout: DesktopLayout) => void;
  vi.mocked(invoke).mockImplementation(() => new Promise<DesktopLayout>(resolve => { restore = resolve; }));
  start();
  expect(screen.getByRole("status", { name: "Carregando Jarvis" })).toBeVisible();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  await act(async () => restore({ ...saved, lastSeenReleaseVersion: APP_VERSION }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("still dismisses the dialog when preferences cannot be saved", async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === "get_desktop_layout") return saved;
    throw new Error("Disk unavailable");
  });
  start();
  await userEvent.click(await screen.findByRole("button", { name: "Continuar" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(saved.lastSeenReleaseVersion).toBeNull();
});
